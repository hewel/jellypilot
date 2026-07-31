/**
 * Route-owned Library Browser paging and cache module for both browse modes.
 *
 * Owns page-zero bootstrap, the normal-grid/virtual-mode decision,
 * random-access virtual window fetching, per-page cache bridging, identity
 * reset, and failed-page retry. The pure layout and page-selection utilities
 * remain internal seams; the route keeps rendering and (for now) geometry.
 */
import type {
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPage,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '@bindings';
import { createInfiniteQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit } from 'effect';
import { createEffect, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { LIBRARY_BROWSE_PAGE_SIZE, fetchVideoLibraryPage } from '~effects/library';
import type { LibraryBrowseState, LibraryExit } from '~effects/library';
import { queryKeys, runExit } from '~effects/query';
import type { LibrarySessionKey } from '~effects/query';
import { libraryBrowsePageLocationForDisplayIndex } from '~utils/libraryBrowsePageSelection';

export const LIBRARY_VIRTUAL_TOTAL_THRESHOLD = 100;

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
  /** Page starts covering the current virtual window plus look-ahead. */
  readonly virtualPageStartsForCurrentWindow: () => readonly number[];
  /** Route-level reaction to identity changes (scroll reset). */
  readonly onIdentityReset: () => void;
}

type LibraryBrowsePageFailure = LibraryExit<LibraryBrowseState> & { _tag: 'Failure' };

export interface LibraryBrowseWindow {
  readonly readyState: () => { items: VideoLibraryItem[]; page: VideoLibraryPage } | null;
  readonly totalRecordCount: () => number;
  /** True for libraries large enough to use random-access virtual paging. */
  readonly usesVirtualGrid: () => boolean;
  readonly needsReverse: () => boolean;
  readonly firstPage: () => LibraryExit<LibraryBrowseState> | null;
  readonly laterPageFailure: () => { index: number; page: LibraryBrowsePageFailure } | null;
  readonly isPending: () => boolean;
  readonly isFetching: () => boolean;
  readonly isFetchingNextPage: () => boolean;
  readonly hasNextPage: () => boolean;
  readonly fetchNextPage: () => void;
  readonly itemForDisplayIndex: (displayIndex: number) => VideoLibraryItem | null;
  readonly loadedDisplayItemCount: () => number;
  readonly loadMoreErrorDescription: () => string | null;
  readonly loadMoreRetryBusy: () => boolean;
  readonly retryFailedPage: () => void;
}

export function createLibraryBrowseWindow(
  options: LibraryBrowseWindowOptions,
): LibraryBrowseWindow {
  const queryClient = useQueryClient();
  const [virtualPagesByStartIndex, setVirtualPagesByStartIndex] = createSignal(
    new Map<number, LibraryExit<LibraryBrowseState>>(),
  );
  const [virtualPageStartsFetching, setVirtualPageStartsFetching] = createSignal(new Set<number>());

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

  // Identity reset: sort/filter/library/session changes clear virtual state,
  // reject in-flight completions via the signature check, and reset scroll.
  let activeBrowseQuerySignature = '';
  createEffect(() => {
    const nextSignature = browseQuerySignature();
    if (activeBrowseQuerySignature && activeBrowseQuerySignature !== nextSignature) {
      setVirtualPagesByStartIndex(new Map<number, LibraryExit<LibraryBrowseState>>());
      setVirtualPageStartsFetching(new Set<number>());
      options.onIdentityReset();
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
    for (const page of virtualPagesByStartIndex().values()) {
      if (Exit.isSuccess(page)) {
        pages.set(page.value.page.startIndex, page.value);
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
  const loadedDisplayItemCount = () =>
    Math.min(
      totalRecordCount(),
      [...successfulPageMap().values()].reduce((count, page) => count + page.items.length, 0),
    );

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
        queryFn: () =>
          runExit(
            fetchVideoLibraryPage(
              request.collectionType,
              request.libraryId,
              startIndex,
              request.sort,
              request.playedFilter,
              request.favoritesOnly,
            ),
          ),
      })
      .then((page) => {
        if (browseQuerySignature() !== expectedSignature) {
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
  const fetchVisibleVirtualPages = (allowNetworkFetch: boolean) => {
    for (const startIndex of options.virtualPageStartsForCurrentWindow()) {
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

  createEffect(() => {
    if (!usesVirtualGrid() || !canUseVirtualPages()) {
      return;
    }

    fetchVisibleVirtualPages(!browseQuery.isFetching);
  });

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
      fetchVisibleVirtualPages(true);
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

  return {
    readyState,
    totalRecordCount,
    usesVirtualGrid,
    needsReverse,
    firstPage,
    laterPageFailure,
    isPending: () => browseQuery.isPending,
    isFetching: () => browseQuery.isFetching,
    isFetchingNextPage: () => browseQuery.isFetchingNextPage,
    hasNextPage: () => browseQuery.hasNextPage,
    fetchNextPage: () => void browseQuery.fetchNextPage({ cancelRefetch: false }),
    itemForDisplayIndex,
    loadedDisplayItemCount,
    loadMoreErrorDescription,
    loadMoreRetryBusy,
    retryFailedPage,
  };
}
