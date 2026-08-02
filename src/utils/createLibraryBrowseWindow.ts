/**
 * Route-owned Library Browser virtual window module for both browse modes.
 *
 * Owns page-zero bootstrap, the normal-grid/virtual-mode decision,
 * random-access virtual window fetching, per-page cache bridging, identity
 * reset, failed-page retry, and the virtual geometry: shared-viewport
 * measurement, enablement, overscan, canvas height, and row placement. The
 * pure layout and page-selection utilities remain internal seams; the route
 * stays the rendering adapter.
 */
import type {
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '@bindings';
import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { createInfiniteQuery, useQueryClient } from '@tanstack/solid-query';
import { createVirtualizer, observeElementRect } from '@tanstack/solid-virtual';
import type { VirtualItem } from '@tanstack/solid-virtual';
import { Effect, Exit, Semaphore } from 'effect';
import { createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { LIBRARY_BROWSE_PAGE_SIZE, fetchVideoLibraryPage } from '~effects/library';
import type { LibraryBrowseState, LibraryExit } from '~effects/library';
import { queryKeys, runExit } from '~effects/query';
import type { LibrarySessionKey } from '~effects/query';
import {
  libraryBrowseColumnCount,
  libraryBrowseVirtualOverscanRows,
  libraryBrowseVirtualRowHeight,
} from '~utils/libraryBrowseLayout';
import {
  libraryBrowsePageLocationForDisplayIndex,
  libraryBrowsePageStartsForRows,
  retainLibraryBrowsePages,
} from '~utils/libraryBrowsePageSelection';

export const LIBRARY_VIRTUAL_TOTAL_THRESHOLD = 100;
const LIBRARY_VIRTUAL_PAGE_CONCURRENCY = 2;
const LIBRARY_VIRTUAL_PAGE_GC_TIME_MS = 120_000;

interface LibraryBrowseInfiniteData {
  pages: LibraryExit<LibraryBrowseState>[];
  pageParams: number[];
}

/** Query identity for one library browse result set. */
export interface LibraryBrowseWindowRequest {
  readonly sessionKey: LibrarySessionKey;
  readonly collectionType: VideoLibraryKind;
  readonly libraryId: string;
  readonly sort: VideoLibrarySort;
  readonly playedFilter: VideoLibraryPlayedFilter;
  readonly favoritesOnly: boolean;
  readonly sortDirection: 'asc' | 'desc';
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
      readonly loadMoreError: string | null;
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
  const virtualPageFetchSemaphore = Semaphore.makeUnsafe(LIBRARY_VIRTUAL_PAGE_CONCURRENCY);
  const [virtualPagesByStartIndex, setVirtualPagesByStartIndex] = createSignal(
    new Map<number, LibraryExit<LibraryBrowseState>>(),
  );
  const [virtualPageStartsFetching, setVirtualPageStartsFetching] = createSignal(new Set<number>());
  let retainedVirtualPageStarts = new Set<number>();

  const browseQueryKey = () => {
    const request = options.request();
    return queryKeys.libraryBrowse(
      request.sessionKey,
      request.collectionType,
      request.libraryId,
      request.sort,
      request.playedFilter,
      request.favoritesOnly,
      request.sortDirection,
    );
  };
  const browsePageQueryKey = (startIndex: number) => {
    const request = options.request();
    return queryKeys.libraryBrowsePage(
      request.sessionKey,
      request.collectionType,
      request.libraryId,
      request.sort,
      request.playedFilter,
      request.favoritesOnly,
      request.sortDirection,
      startIndex,
    );
  };
  const browseQuerySignature = createMemo(() => JSON.stringify(browseQueryKey()));

  const browseQuery = createInfiniteQuery(() => ({
    queryKey: browseQueryKey(),
    enabled: options.filtersReady() && options.sessionActive(),
    queryFn: ({ pageParam }) => {
      const request = options.request();
      const startIndex = typeof pageParam === 'number' ? pageParam : 0;
      return runExit(
        fetchVideoLibraryPage(
          request.collectionType,
          request.libraryId,
          startIndex,
          request.sort,
          request.playedFilter,
          request.favoritesOnly,
        ),
      );
    },
    initialPageParam: 0,
    getNextPageParam: (lastPage) =>
      Exit.match(lastPage, {
        onFailure: () => undefined,
        onSuccess: (value) =>
          value.page.hasMore ? value.page.startIndex + value.page.limit : undefined,
      }),
  }));

  const appScroll = useAppScrollArea();

  // Identity reset: sort/filter/library/session changes clear virtual state,
  // reject in-flight completions via the signature check, and reset scroll.
  let activeBrowseQuerySignature = '';
  createEffect(() => {
    const nextSignature = browseQuerySignature();
    if (activeBrowseQuerySignature && activeBrowseQuerySignature !== nextSignature) {
      setVirtualPagesByStartIndex(new Map<number, LibraryExit<LibraryBrowseState>>());
      retainedVirtualPageStarts = new Set<number>();
      setVirtualPageStartsFetching(new Set<number>());
      appScroll.scrollTo({ top: 0 });
    }
    activeBrowseQuerySignature = nextSignature;
  });

  const successfulPages = () =>
    browseQuery.data?.pages.filter(
      (page): page is LibraryExit<LibraryBrowseState> & { _tag: 'Success' } => Exit.isSuccess(page),
    ) ?? [];

  // Cache bridging: sequential pages seed the per-page entries the
  // random-access virtual window reuses on cached route re-entry.
  createEffect(() => {
    for (const page of successfulPages()) {
      queryClient.setQueryData(browsePageQueryKey(page.value.page.startIndex), page);
    }
  });

  const successfulPageMap = createMemo(() => {
    const pages = new Map<number, LibraryBrowseState>();
    for (const page of successfulPages()) {
      pages.set(page.value.page.startIndex, page.value);
    }
    for (const [startIndex, page] of virtualPagesByStartIndex()) {
      if (Exit.isSuccess(page)) {
        pages.set(startIndex, page.value);
      }
    }
    return pages;
  });
  const firstPage = () => browseQuery.data?.pages[0] ?? null;
  const laterPageFailure = () => {
    const pages = browseQuery.data?.pages ?? [];
    const index = pages.findIndex((page, pageIndex) => pageIndex > 0 && !Exit.isSuccess(page));
    if (index === -1) {
      return null;
    }
    const page = pages[index];
    return page && !Exit.isSuccess(page) ? { index, page } : null;
  };
  const virtualPageFailure = () => {
    for (const page of virtualPagesByStartIndex().values()) {
      if (!Exit.isSuccess(page)) {
        return page;
      }
    }
    return null;
  };
  const needsReverse = () => {
    const request = options.request();
    const isDefaultAsc = request.sort === 'title';
    return isDefaultAsc ? request.sortDirection === 'desc' : request.sortDirection === 'asc';
  };
  const readyState = () => {
    const pages = successfulPages();
    if (pages.length === 0) {
      return null;
    }
    const last = pages[pages.length - 1]?.value;
    if (!last) {
      return null;
    }
    const items = pages.flatMap((page) => page.value.items);

    return {
      items: needsReverse() ? [...items].toReversed() : items,
      page: last.page,
    };
  };
  const totalRecordCount = () => readyState()?.page.totalRecordCount ?? 0;
  const usesVirtualGrid = () => totalRecordCount() > LIBRARY_VIRTUAL_TOTAL_THRESHOLD;
  const itemForDisplayIndex = (displayIndex: number) => {
    const location = libraryBrowsePageLocationForDisplayIndex({
      displayIndex,
      totalRecordCount: totalRecordCount(),
      pageSize: LIBRARY_BROWSE_PAGE_SIZE,
      reverse: needsReverse(),
    });
    if (!location) {
      return null;
    }

    const page = successfulPageMap().get(location.pageStart);
    return page?.items[location.indexWithinPage] ?? null;
  };

  const fetchVirtualPage = (startIndex: number, allowNetworkFetch: boolean) => {
    const total = totalRecordCount();
    if (
      startIndex < 0 ||
      startIndex >= total ||
      successfulPageMap().has(startIndex) ||
      virtualPagesByStartIndex().has(startIndex) ||
      virtualPageStartsFetching().has(startIndex)
    ) {
      return;
    }

    const request = options.request();
    const expectedSignature = browseQuerySignature();
    const virtualPageQueryKey = browsePageQueryKey(startIndex);
    const cachedPage =
      queryClient.getQueryData<LibraryExit<LibraryBrowseState>>(virtualPageQueryKey);
    if (cachedPage && Exit.isSuccess(cachedPage)) {
      setVirtualPagesByStartIndex((current) => new Map([...current, [startIndex, cachedPage]]));
      return;
    }

    if (!allowNetworkFetch) {
      return;
    }

    setVirtualPageStartsFetching((current) => new Set([...current, startIndex]));

    void queryClient
      .fetchQuery({
        queryKey: virtualPageQueryKey,
        gcTime: LIBRARY_VIRTUAL_PAGE_GC_TIME_MS,
        queryFn: () =>
          runExit(
            virtualPageFetchSemaphore.withPermit(
              Effect.suspend(() =>
                browseQuerySignature() === expectedSignature &&
                retainedVirtualPageStarts.has(startIndex)
                  ? fetchVideoLibraryPage(
                      request.collectionType,
                      request.libraryId,
                      startIndex,
                      request.sort,
                      request.playedFilter,
                      request.favoritesOnly,
                    )
                  : Effect.interrupt,
              ),
            ),
          ),
      })
      .then((page) => {
        if (
          browseQuerySignature() !== expectedSignature ||
          !retainedVirtualPageStarts.has(startIndex)
        ) {
          return;
        }

        setVirtualPagesByStartIndex((current) => new Map([...current, [startIndex, page]]));
      })
      .finally(() => {
        if (browseQuerySignature() !== expectedSignature) {
          return;
        }

        setVirtualPageStartsFetching((current) => {
          const next = new Set(current);
          next.delete(startIndex);
          return next;
        });
      });
  };
  const fetchVisibleVirtualPages = (
    pageStarts: ReadonlySet<number>,
    allowNetworkFetch: boolean,
  ) => {
    for (const startIndex of pageStarts) {
      fetchVirtualPage(startIndex, allowNetworkFetch);
    }
  };
  const canUseVirtualPages = () => {
    const currentFirstPage = firstPage();

    return (
      options.filtersReady() &&
      options.sessionActive() &&
      currentFirstPage !== null &&
      Exit.isSuccess(currentFirstPage) &&
      currentFirstPage.value.page.startIndex === 0
    );
  };

  const loadMoreRetryBusy = () =>
    usesVirtualGrid() ? virtualPageStartsFetching().size > 0 : browseQuery.isFetchingNextPage;
  const loadMoreErrorDescription = () => {
    const virtualFailure = usesVirtualGrid() ? virtualPageFailure() : null;
    if (virtualFailure) {
      return commandFailureMessage(virtualFailure.cause, 'Could not load Library page');
    }

    const failure = laterPageFailure();
    return failure
      ? commandFailureMessage(failure.page.cause, 'Could not load Library page')
      : null;
  };
  const retryFailedPage = () => {
    if (usesVirtualGrid()) {
      const failedStarts = [...virtualPagesByStartIndex().entries()]
        .filter(([, page]) => !Exit.isSuccess(page))
        .map(([startIndex]) => startIndex);
      if (failedStarts.length === 0 || virtualPageStartsFetching().size > 0) {
        return;
      }

      setVirtualPagesByStartIndex((current) => {
        const next = new Map(current);
        for (const startIndex of failedStarts) {
          next.delete(startIndex);
        }
        return next;
      });
      fetchVisibleVirtualPages(retainedVirtualPageStarts, true);
      return;
    }

    const failure = laterPageFailure();
    if (!failure || browseQuery.isFetching) {
      return;
    }
    queryClient.setQueryData<LibraryBrowseInfiniteData>(browseQueryKey(), (data) => {
      if (!data) {
        return data;
      }
      return {
        pages: data.pages.filter((_, index) => index !== failure.index),
        pageParams: data.pageParams.filter((_, index) => index !== failure.index),
      };
    });
    void browseQuery.fetchNextPage({ cancelRefetch: false });
  };
  const loadNextPage = () => {
    if (
      usesVirtualGrid() ||
      !browseQuery.hasNextPage ||
      browseQuery.isFetching ||
      laterPageFailure()
    ) {
      return;
    }
    void browseQuery.fetchNextPage({ cancelRefetch: false });
  };
  const state = createMemo<LibraryBrowseViewState>(() => {
    if (!options.filtersReady() || browseQuery.isPending) {
      return { tag: 'loading' };
    }

    const currentFirstPage = firstPage();
    if (!currentFirstPage) {
      return { tag: 'loading' };
    }
    if (!Exit.isSuccess(currentFirstPage)) {
      return {
        tag: 'initialError',
        message: commandFailureMessage(currentFirstPage.cause, 'Could not load Library page'),
      };
    }
    if (currentFirstPage.value.items.length === 0) {
      return {
        tag: 'empty',
        totalRecordCount: currentFirstPage.value.page.totalRecordCount,
      };
    }

    const currentReadyState = readyState();
    if (!currentReadyState) {
      return { tag: 'loading' };
    }
    const mode = usesVirtualGrid() ? 'virtual' : 'normal';
    return {
      tag: 'ready',
      mode,
      items: currentReadyState.items,
      totalRecordCount: currentReadyState.page.totalRecordCount,
      isFetchingMore:
        mode === 'virtual' ? virtualPageStartsFetching().size > 0 : browseQuery.isFetchingNextPage,
      loadMoreError: loadMoreErrorDescription(),
      retryBusy: loadMoreRetryBusy(),
    };
  });

  // Virtual geometry: shared-viewport measurement, enablement, overscan,
  // canvas height, and row placement behind one interface.
  const [virtualGrid, setVirtualGrid] = createSignal<HTMLDivElement | null>(null);
  const [virtualGridWidth, setVirtualGridWidth] = createSignal(1280);
  const [virtualViewportHeight, setVirtualViewportHeight] = createSignal(720);
  const [virtualizerMounted, setVirtualizerMounted] = createSignal(false);
  const [virtualScrollMargin, setVirtualScrollMargin] = createSignal(0);
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
  });
  const virtualPageStartsForCurrentWindow = () =>
    libraryBrowsePageStartsForRows({
      rowIndexes: rowVirtualizer.getVirtualItems().map((virtualRow) => virtualRow.index),
      columnCount: columnCount(),
      totalRecordCount: totalRecordCount(),
      pageSize: LIBRARY_BROWSE_PAGE_SIZE,
      reverse: needsReverse(),
    });
  createEffect(() => {
    if (!usesVirtualGrid() || !canUseVirtualPages()) {
      retainedVirtualPageStarts = new Set<number>();
      setVirtualPagesByStartIndex((current) => retainLibraryBrowsePages(current, new Set()));
      return;
    }

    retainedVirtualPageStarts = new Set(virtualPageStartsForCurrentWindow());
    setVirtualPagesByStartIndex((current) =>
      retainLibraryBrowsePages(current, retainedVirtualPageStarts),
    );
    fetchVisibleVirtualPages(retainedVirtualPageStarts, !browseQuery.isFetching);
  });

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
