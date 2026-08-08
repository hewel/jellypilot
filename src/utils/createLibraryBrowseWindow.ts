/**
 * Route-owned Library Browser virtual window module for both browse modes.
 *
 * Interprets the portable WASM browse core's metadata commands through
 * Effect-backed page fetches and TanStack's per-page item cache. Solid still
 * owns the route lifecycle and the virtual geometry: shared-viewport
 * measurement, enablement, overscan, canvas height, and row placement.
 */
import type {
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
  VideoLibrarySortDirection,
} from '@bindings';
import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { useQueryClient } from '@tanstack/solid-query';
import { createVirtualizer, observeElementRect } from '@tanstack/solid-virtual';
import type { VirtualItem } from '@tanstack/solid-virtual';
import { Effect, Exit, Match } from 'effect';
import {
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchVideoLibraryPage } from '~effects/library';
import type { LibraryBrowseState, LibraryExit } from '~effects/library';
import { queryKeys, runExit } from '~effects/query';
import type { LibrarySessionKey } from '~effects/query';

import {
  libraryBrowseColumnCount,
  libraryBrowseVirtualOverscanRows,
  libraryBrowseVirtualRowHeight,
} from './libraryBrowseLayout';
import {
  loadLibraryBrowseCore,
  type LibraryBrowseCommand,
  type LibraryBrowseEvent,
  type LibraryBrowseLoadToken,
  type LibraryBrowseSnapshot,
  type LibraryBrowseStatus,
  type LibraryBrowseUpdate,
} from './libraryBrowseWasm';

const LIBRARY_VIRTUAL_PAGE_GC_TIME_MS = 120_000;

/** Query identity for one library browse result set. */
export interface LibraryBrowseWindowRequest {
  readonly sessionKey: LibrarySessionKey;
  readonly collectionType: VideoLibraryKind;
  readonly libraryId: string;
  readonly sort: VideoLibrarySort;
  readonly playedFilter: VideoLibraryPlayedFilter;
  readonly favoritesOnly: boolean;
  readonly sortDirection: VideoLibrarySortDirection;
}

export interface LibraryBrowseWindowOptions {
  /** Current query identity; changes reset virtual paging and scroll. */
  readonly request: () => LibraryBrowseWindowRequest;
  /** Shared filter store has hydrated from persistence. */
  readonly filtersReady: () => boolean;
  /** The mounted session still matches the current connection identity. */
  readonly sessionActive: () => boolean;
}

export type LibraryBrowseViewState =
  | { readonly tag: 'loading' }
  | { readonly tag: 'empty'; readonly totalRecordCount: number }
  | { readonly tag: 'initialError'; readonly message: string }
  | {
      readonly tag: 'ready';
      readonly mode: 'normal' | 'virtual';
      readonly items: VideoLibraryItem[];
      readonly totalRecordCount: number;
      readonly isFetchingMore: boolean;
      readonly canLoadNext: boolean;
      readonly loadMoreError: string | null;
      readonly loadMoreRetryable: boolean;
      readonly retryBusy: boolean;
    };

export interface LibraryBrowseWindow {
  readonly state: () => LibraryBrowseViewState;
  readonly loadNextPage: () => void;
  readonly itemForDisplayIndex: (displayIndex: number) => VideoLibraryItem | null;
  readonly retry: () => void;
  /** Virtual geometry owned behind one interface; the route only renders it. */
  readonly virtual: LibraryBrowseVirtualGeometry;
}

export interface LibraryBrowseVirtualGeometry {
  /** Ref callback for the virtual root element. */
  readonly ref: (element: HTMLDivElement | null) => void;
  /** Rows currently rendered by the virtualizer. */
  readonly items: () => VirtualItem[];
  /** Full virtual canvas height in px. */
  readonly totalSize: () => number;
  /** Scroll offset of the virtual root inside the shared viewport. */
  readonly scrollMargin: () => number;
  readonly columnCount: () => number;
  readonly rowColumnIndexes: () => readonly number[];
}

export function createLibraryBrowseWindow(
  options: LibraryBrowseWindowOptions,
): LibraryBrowseWindow {
  const queryClient = useQueryClient();
  const appScroll = useAppScrollArea();
  let disposed = false;
  let resolvedCore: Awaited<ReturnType<typeof loadLibraryBrowseCore>> | null = null;
  const [core] = createResource(async () => {
    const loadedCore = await loadLibraryBrowseCore();
    if (disposed) {
      loadedCore.free();
    } else {
      resolvedCore = loadedCore;
    }
    return loadedCore;
  });
  const [coreSnapshot, setCoreSnapshot] = createSignal<LibraryBrowseSnapshot | null>(null);
  const [coreRuntimeError, setCoreRuntimeError] = createSignal<string | null>(null);
  const [pagesByStartIndex, setPagesByStartIndex] = createSignal(
    new Map<number, LibraryBrowseState>(),
  );
  const cancelledLoads = new Set<string>();
  const pendingCoreEvents: LibraryBrowseEvent[] = [];
  let activeRequest: LibraryBrowseWindowRequest | null = null;
  let activeSourceId = '';
  let coreDispatchScheduled = false;

  const browseQueryKey = (request: LibraryBrowseWindowRequest) =>
    queryKeys.libraryBrowse(
      request.sessionKey,
      request.collectionType,
      request.libraryId,
      request.sort,
      request.playedFilter,
      request.favoritesOnly,
      request.sortDirection,
    );
  const browsePageQueryKey = (request: LibraryBrowseWindowRequest, startIndex: number) =>
    queryKeys.libraryBrowsePage(
      request.sessionKey,
      request.collectionType,
      request.libraryId,
      request.sort,
      request.playedFilter,
      request.favoritesOnly,
      request.sortDirection,
      startIndex,
    );
  const loadTokenKey = (token: LibraryBrowseLoadToken) => `${token.generation}:${token.sequence}`;

  function applyCoreUpdate(update: LibraryBrowseUpdate) {
    if (disposed) {
      return;
    }
    setCoreRuntimeError(null);
    setCoreSnapshot(update.snapshot);
    for (const command of update.commands) {
      interpretCoreCommand(command);
    }
  }

  function scheduleNextCoreEvent() {
    if (coreDispatchScheduled) {
      return;
    }
    coreDispatchScheduled = true;
    queueMicrotask(() => {
      coreDispatchScheduled = false;
      const event = pendingCoreEvents.shift();
      if (!event || disposed) {
        pendingCoreEvents.length = 0;
        return;
      }

      const currentCore = resolvedCore;
      if (!currentCore) {
        pendingCoreEvents.length = 0;
        return;
      }

      const update = Effect.runSync(
        Effect.try({
          try: () => currentCore.dispatch(event),
          catch: (cause) =>
            cause instanceof Error && cause.message
              ? cause.message
              : 'The Library browse core rejected an adapter event',
        }).pipe(
          Effect.match({
            onFailure: (message) => {
              setCoreRuntimeError(message);
              return null;
            },
            onSuccess: (value) => value,
          }),
        ),
      );
      if (update) {
        applyCoreUpdate(update);
      }
      if (pendingCoreEvents.length > 0) {
        scheduleNextCoreEvent();
      }
    });
  }

  function dispatchCoreEvent(event: LibraryBrowseEvent) {
    if (disposed) {
      return;
    }
    pendingCoreEvents.push(event);
    scheduleNextCoreEvent();
  }

  function settleLoadedPage(
    token: LibraryBrowseLoadToken,
    sourceId: string,
    request: LibraryBrowseWindowRequest,
    command: Extract<LibraryBrowseCommand, { tag: 'loadPage' }>,
    value: LibraryBrowseState,
  ) {
    const tokenKey = loadTokenKey(token);
    const cancelled = cancelledLoads.delete(tokenKey);
    if (disposed) {
      return;
    }
    const endIndex = value.page.startIndex + value.items.length;
    const cachedPageZero = queryClient.getQueryData<LibraryExit<LibraryBrowseState>>(
      browsePageQueryKey(request, 0),
    );
    const pageZeroTotal =
      cachedPageZero && Exit.isSuccess(cachedPageZero)
        ? cachedPageZero.value.page.totalRecordCount
        : undefined;
    const metadataMatchesCommand =
      value.page.startIndex === command.startIndex &&
      value.page.limit === command.limit &&
      value.items.length <= command.limit &&
      value.page.startIndex <= value.page.totalRecordCount &&
      endIndex <= value.page.totalRecordCount &&
      value.page.hasMore === endIndex < value.page.totalRecordCount &&
      (command.startIndex === 0 ||
        pageZeroTotal === undefined ||
        pageZeroTotal === value.page.totalRecordCount);
    if (!metadataMatchesCommand) {
      queryClient.removeQueries({
        queryKey: browsePageQueryKey(request, command.startIndex),
        exact: true,
      });
    } else if (!cancelled && sourceId === activeSourceId) {
      setPagesByStartIndex((current) => new Map([...current, [command.startIndex, value]]));
    }
    dispatchCoreEvent({
      tag: 'pageSettled',
      token,
      outcome: {
        tag: 'loaded',
        startIndex: value.page.startIndex,
        limit: value.page.limit,
        totalRecordCount: value.page.totalRecordCount,
        itemCount: value.items.length,
        hasMore: value.page.hasMore,
      },
    });
  }

  function settleFailedPage(token: LibraryBrowseLoadToken, message: string) {
    cancelledLoads.delete(loadTokenKey(token));
    if (disposed) {
      return;
    }
    dispatchCoreEvent({
      tag: 'pageSettled',
      token,
      outcome: { tag: 'failed', failure: { message, retryable: true } },
    });
  }

  function loadPage(command: Extract<LibraryBrowseCommand, { tag: 'loadPage' }>) {
    const request = activeRequest;
    const sourceId = activeSourceId;
    if (!request || !sourceId) {
      return;
    }

    const queryKey = browsePageQueryKey(request, command.startIndex);
    const cachedPage = queryClient.getQueryData<LibraryExit<LibraryBrowseState>>(queryKey);
    if (command.cacheMode === 'reuseSuccess' && cachedPage && Exit.isSuccess(cachedPage)) {
      settleLoadedPage(command.token, sourceId, request, command, cachedPage.value);
      return;
    }
    if (cachedPage || command.cacheMode === 'reload') {
      queryClient.removeQueries({ queryKey, exact: true });
    }
    if (command.cacheMode === 'reload') {
      setPagesByStartIndex((current) => {
        const next = new Map(current);
        next.delete(command.startIndex);
        return next;
      });
    }

    void queryClient
      .fetchQuery({
        queryKey,
        gcTime: LIBRARY_VIRTUAL_PAGE_GC_TIME_MS,
        queryFn: () =>
          runExit(
            fetchVideoLibraryPage(
              request.collectionType,
              request.libraryId,
              command.startIndex,
              command.limit,
              request.sort,
              request.playedFilter,
              request.favoritesOnly,
              request.sortDirection,
            ),
          ),
      })
      .then((page) =>
        Exit.match(page, {
          onFailure: (cause) =>
            settleFailedPage(
              command.token,
              commandFailureMessage(cause, 'Could not load Library page'),
            ),
          onSuccess: (value) => settleLoadedPage(command.token, sourceId, request, command, value),
        }),
      )
      .catch((error: unknown) =>
        settleFailedPage(
          command.token,
          error instanceof Error && error.message ? error.message : 'Could not load Library page',
        ),
      );
  }

  const interpretCoreCommand = Match.type<LibraryBrowseCommand>().pipe(
    Match.when({ tag: 'resetViewport' }, () => appScroll.scrollTo({ top: 0 })),
    Match.when({ tag: 'loadPage' }, loadPage),
    Match.when({ tag: 'cancelLoad' }, (command) => {
      cancelledLoads.add(loadTokenKey(command.token));
    }),
    Match.when({ tag: 'releasePages' }, (command) => {
      const releasedStarts = new Set(command.pageStarts);
      setPagesByStartIndex((current) => {
        const next = new Map(current);
        for (const pageStart of releasedStarts) {
          next.delete(pageStart);
        }
        return next;
      });
    }),
    Match.exhaustive,
  );

  createEffect(() => {
    const loading = core.loading;
    const initializationError = core.error;
    const currentCore = resolvedCore;
    const request = options.request();
    const sourceId = JSON.stringify(browseQueryKey(request));
    const enabled = options.filtersReady() && options.sessionActive();
    if (loading || initializationError || !currentCore) {
      return;
    }

    if (sourceId !== activeSourceId) {
      activeSourceId = sourceId;
      setPagesByStartIndex(new Map<number, LibraryBrowseState>());
    }
    activeRequest = request;
    dispatchCoreEvent({ tag: 'configure', sourceId, enabled });
  });

  onCleanup(() => {
    disposed = true;
    resolvedCore?.free();
    resolvedCore = null;
  });

  const slotByDisplayIndex = createMemo(
    () => new Map(coreSnapshot()?.slots.map((slot) => [slot.displayIndex, slot])),
  );
  const itemForDisplayIndex = (displayIndex: number) => {
    const slot = slotByDisplayIndex().get(displayIndex);
    if (!slot) {
      return null;
    }
    return pagesByStartIndex().get(slot.pageStart)?.items[slot.indexWithinPage] ?? null;
  };
  const readyItems = () =>
    (coreSnapshot()?.slots ?? []).flatMap((slot) => {
      const item = pagesByStartIndex().get(slot.pageStart)?.items[slot.indexWithinPage];
      return item ? [item] : [];
    });
  const stateForStatus = Match.type<LibraryBrowseStatus>().pipe(
    Match.when({ tag: 'inactive' }, (): LibraryBrowseViewState => ({ tag: 'loading' })),
    Match.when({ tag: 'loading' }, (): LibraryBrowseViewState => ({ tag: 'loading' })),
    Match.when(
      { tag: 'empty' },
      (status): LibraryBrowseViewState => ({
        tag: 'empty',
        totalRecordCount: status.totalRecordCount,
      }),
    ),
    Match.when(
      { tag: 'initialFailure' },
      (status): LibraryBrowseViewState => ({
        tag: 'initialError',
        message: status.failure.message,
      }),
    ),
    Match.when(
      { tag: 'ready' },
      (status): LibraryBrowseViewState => ({
        tag: 'ready',
        mode: status.mode,
        items: status.mode === 'normal' ? readyItems() : [],
        totalRecordCount: status.totalRecordCount,
        isFetchingMore: status.isFetchingMore,
        canLoadNext: status.canLoadNext,
        loadMoreError: status.loadMoreFailure?.message ?? null,
        loadMoreRetryable: status.loadMoreFailure?.retryable ?? false,
        retryBusy: status.retryBusy,
      }),
    ),
    Match.exhaustive,
  );
  const state = createMemo<LibraryBrowseViewState>(() => {
    const runtimeError = coreRuntimeError();
    if (runtimeError) {
      return { tag: 'initialError', message: runtimeError };
    }
    if (core.error) {
      return {
        tag: 'initialError',
        message:
          core.error instanceof Error && core.error.message
            ? core.error.message
            : 'Could not initialize Library browser',
      };
    }
    const currentSnapshot = coreSnapshot();
    return currentSnapshot ? stateForStatus(currentSnapshot.status) : { tag: 'loading' };
  });
  const virtualReadyStatus = () => {
    const status = coreSnapshot()?.status;
    return status?.tag === 'ready' && status.mode === 'virtual' ? status : null;
  };
  const totalRecordCount = () => virtualReadyStatus()?.totalRecordCount ?? 0;
  const usesVirtualGrid = () => virtualReadyStatus() !== null;

  // Virtual geometry: shared-viewport measurement, enablement, overscan,
  // canvas height, and row placement behind one interface.
  const [virtualGrid, setVirtualGrid] = createSignal<HTMLDivElement | null>(null);
  const [virtualGridWidth, setVirtualGridWidth] = createSignal(1280);
  const [virtualViewportHeight, setVirtualViewportHeight] = createSignal(720);
  const [virtualizerMounted, setVirtualizerMounted] = createSignal(false);
  const [virtualScrollMargin, setVirtualScrollMargin] = createSignal(0);
  const [virtualWindowRevision, setVirtualWindowRevision] = createSignal(0);
  onMount(() => setVirtualizerMounted(true));

  const fallbackVirtualGridWidth = () => {
    const gridWidth = virtualGrid()?.clientWidth ?? 0;
    if (gridWidth > 0) {
      return gridWidth;
    }

    const viewportWidth = appScroll.viewport()?.clientWidth ?? 0;
    if (viewportWidth > 0) {
      return viewportWidth;
    }

    if (typeof window !== 'undefined' && window.innerWidth > 0) {
      return window.innerWidth;
    }

    return 1280;
  };
  const fallbackVirtualGridHeight = () => {
    const viewportHeight = appScroll.viewport()?.clientHeight ?? 0;
    if (viewportHeight > 0) {
      return viewportHeight;
    }

    if (typeof window !== 'undefined' && window.innerHeight > 0) {
      return window.innerHeight;
    }

    return 720;
  };
  const measureVirtualGrid = () => {
    setVirtualGridWidth(fallbackVirtualGridWidth());
    setVirtualViewportHeight(fallbackVirtualGridHeight());

    const grid = virtualGrid();
    const scrollElement = appScroll.viewport();
    if (!grid || !scrollElement) {
      setVirtualScrollMargin(0);
      return;
    }

    setVirtualScrollMargin(
      grid.getBoundingClientRect().top -
        scrollElement.getBoundingClientRect().top +
        scrollElement.scrollTop,
    );
  };
  createEffect(() => {
    const grid = virtualGrid();
    const scrollElement = appScroll.viewport();
    if (typeof ResizeObserver === 'undefined') {
      measureVirtualGrid();
      if (typeof window !== 'undefined') {
        window.addEventListener('resize', measureVirtualGrid);
        onCleanup(() => window.removeEventListener('resize', measureVirtualGrid));
      }
      return;
    }

    const observer = new ResizeObserver(measureVirtualGrid);
    if (grid) {
      observer.observe(grid);
    }
    if (scrollElement) {
      observer.observe(scrollElement);
    }
    onCleanup(() => observer.disconnect());
  });

  const columnCount = createMemo(() => libraryBrowseColumnCount(virtualGridWidth()));
  const virtualRowColumnIndexes = createMemo(() =>
    Array.from({ length: columnCount() }, (_, index) => index),
  );
  const estimateVirtualRowHeight = () => libraryBrowseVirtualRowHeight(virtualGridWidth());
  const rowVirtualizer = createVirtualizer<HTMLElement, HTMLDivElement>({
    get count() {
      return usesVirtualGrid() ? Math.ceil(totalRecordCount() / columnCount()) : 0;
    },
    get enabled() {
      return virtualizerMounted() && usesVirtualGrid() && appScroll.viewport() !== null;
    },
    getScrollElement: () => appScroll.viewport(),
    estimateSize: estimateVirtualRowHeight,
    get overscan() {
      return libraryBrowseVirtualOverscanRows(virtualViewportHeight(), estimateVirtualRowHeight());
    },
    observeElementRect: (instance, callback) =>
      observeElementRect(instance, (rect) =>
        callback({
          width: rect.width || virtualGridWidth(),
          height: rect.height || virtualViewportHeight(),
        }),
      ),
    get initialRect() {
      return { width: virtualGridWidth(), height: virtualViewportHeight() };
    },
    get scrollMargin() {
      return virtualScrollMargin();
    },
    onChange: () => setVirtualWindowRevision((revision) => revision + 1),
  });
  const virtualDisplayIndexesForCurrentWindow = () => {
    const indexes: number[] = [];
    const columns = columnCount();
    const total = totalRecordCount();
    for (const virtualRow of rowVirtualizer.getVirtualItems()) {
      for (let columnIndex = 0; columnIndex < columns; columnIndex += 1) {
        const displayIndex = virtualRow.index * columns + columnIndex;
        if (displayIndex < total) {
          indexes.push(displayIndex);
        }
      }
    }
    return indexes;
  };
  let lastVirtualWindowSignature = '';
  createEffect(() => {
    virtualWindowRevision();
    const loading = core.loading;
    const initializationError = core.error;
    if (loading || initializationError || !resolvedCore || !activeSourceId) {
      return;
    }
    const displayIndexes = usesVirtualGrid() ? virtualDisplayIndexesForCurrentWindow() : [];
    const signature = `${activeSourceId}:${displayIndexes.join(',')}`;
    if (signature === lastVirtualWindowSignature) {
      return;
    }
    lastVirtualWindowSignature = signature;
    dispatchCoreEvent({ tag: 'windowChanged', displayIndexes });
  });

  const loadNextPage = () => dispatchCoreEvent({ tag: 'loadNext' });
  const retryFailedPage = () => dispatchCoreEvent({ tag: 'retry' });

  const virtual: LibraryBrowseVirtualGeometry = {
    ref: setVirtualGrid,
    items: () => rowVirtualizer.getVirtualItems(),
    totalSize: () => rowVirtualizer.getTotalSize(),
    scrollMargin: virtualScrollMargin,
    columnCount,
    rowColumnIndexes: virtualRowColumnIndexes,
  };

  return {
    state,
    loadNextPage,
    itemForDisplayIndex,
    retry: retryFailedPage,
    virtual,
  };
}
