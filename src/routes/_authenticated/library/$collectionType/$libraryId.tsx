import { Menu } from '@ark-ui/solid/menu';
import { Toggle } from '@ark-ui/solid/toggle';
import type { VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibrarySort } from '@bindings';
import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { useLibraryNavbarControls } from '@components/library/LibraryNavbarContext';
import {
  LibraryStatusPanel,
  VideoCard,
  libraryTitle,
  playedFilterLabel,
  sortItems,
} from '@components/library/shared';
import { Button } from '@components/ui';
import { createInfiniteQuery, createQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, useNavigate } from '@tanstack/solid-router';
import { createVirtualizer, observeElementRect } from '@tanstack/solid-virtual';
import { Exit } from 'effect';
import {
  Check,
  RefreshCw,
  ListSortAscending,
  Funnel,
  ArrowDownWideNarrowIcon,
  ArrowUpWideNarrowIcon,
} from 'lucide-solid';
import {
  For,
  Show,
  Suspense,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from 'solid-js';
import { Portal } from 'solid-js/web';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import { LIBRARY_BROWSE_PAGE_SIZE, fetchVideoLibraryPage } from '~effects/library';
import type { LibraryBrowseState, LibraryExit } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';
import { createSharedLibraryFilters } from '~utils/createSharedLibraryFilters';
import type { LibrarySortDirection } from '~utils/createSharedLibraryFilters';
import {
  LIBRARY_BROWSE_AUTO_GRID_CLASS,
  LIBRARY_BROWSE_GRID_GAP_PX,
  libraryBrowseColumnCount,
} from '~utils/libraryBrowseLayout';

const LIBRARY_BROWSE_SKELETON_CARD_KEYS = Array.from({ length: 10 }, (_, index) => index);
const LIBRARY_VIRTUAL_TOTAL_THRESHOLD = 100;
const LIBRARY_BROWSE_GRID_OVERSCAN_ROWS = 3;

interface LibraryBrowseInfiniteData {
  pages: LibraryExit<LibraryBrowseState>[];
  pageParams: number[];
}

function collectionTypeFromParam(collectionType: string): VideoLibraryKind {
  return collectionType === 'tvshows' ? 'tvshows' : 'movies';
}

export const Route = createFileRoute('/_authenticated/library/$collectionType/$libraryId')({
  component: LibraryBrowseRoute,
});

function LibraryBrowseRoute() {
  const params = Route.useParams();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const libraryFilters = createSharedLibraryFilters();
  const filterSort = libraryFilters.sort;
  const [autoLoadSentinel, setAutoLoadSentinel] = createSignal<HTMLDivElement | null>(null);
  const [autoLoadSentinelVisible, setAutoLoadSentinelVisible] = createSignal(false);
  const [virtualGrid, setVirtualGrid] = createSignal<HTMLDivElement | null>(null);
  const [virtualGridWidth, setVirtualGridWidth] = createSignal(1280);
  const appScroll = useAppScrollArea();
  const [virtualScrollMargin, setVirtualScrollMargin] = createSignal(0);
  const [virtualPagesByStartIndex, setVirtualPagesByStartIndex] = createSignal(
    new Map<number, LibraryExit<LibraryBrowseState>>(),
  );
  const [virtualPageStartsFetching, setVirtualPageStartsFetching] = createSignal(new Set<number>());
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState()),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const activeSessionSignature = createMemo(() => {
    const currentSessionKey = sessionKey();
    return isLibrarySessionKeyConnected(currentSessionKey)
      ? `${currentSessionKey.provider}\u0000${currentSessionKey.serverUrl}\u0000${currentSessionKey.userId}`
      : null;
  });
  const [redirectingForSessionChange, setRedirectingForSessionChange] = createSignal(false);
  let mountedSessionSignature: string | null = null;
  const isMountedSessionActive = () => {
    const currentSessionSignature = activeSessionSignature();
    return (
      currentSessionSignature !== null &&
      !redirectingForSessionChange() &&
      (mountedSessionSignature === null || mountedSessionSignature === currentSessionSignature)
    );
  };

  createEffect(() => {
    if (redirectingForSessionChange()) {
      return;
    }

    const currentSessionSignature = activeSessionSignature();
    if (currentSessionSignature === null) {
      if (mountedSessionSignature !== null) {
        setRedirectingForSessionChange(true);
        void navigate({ to: '/library', replace: true });
      }
      return;
    }

    if (mountedSessionSignature === null) {
      mountedSessionSignature = currentSessionSignature;
      return;
    }

    if (mountedSessionSignature !== currentSessionSignature) {
      setRedirectingForSessionChange(true);
      void navigate({ to: '/library', replace: true });
    }
  });

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

  onMount(() => {
    measureVirtualGrid();

    if (typeof window === 'undefined') {
      return;
    }

    window.addEventListener('resize', measureVirtualGrid);
    onCleanup(() => window.removeEventListener('resize', measureVirtualGrid));
  });

  createEffect(() => {
    virtualGrid();
    appScroll.viewport();
    measureVirtualGrid();
  });

  createEffect(() => {
    const grid = virtualGrid();
    const scrollElement = appScroll.viewport();
    if (typeof ResizeObserver === 'undefined') {
      measureVirtualGrid();
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

  const collectionType = () => collectionTypeFromParam(params().collectionType);
  const browseQueryKey = () =>
    queryKeys.libraryBrowse(
      sessionKey(),
      collectionType(),
      params().libraryId,
      filterSort(),
      libraryFilters.playedFilter(),
      libraryFilters.favoritesOnly(),
      libraryFilters.sortDirection(),
    );
  const browseQuery = createInfiniteQuery(() => ({
    queryKey: browseQueryKey(),
    enabled: libraryFilters.ready() && isMountedSessionActive(),
    queryFn: ({ pageParam }) => {
      const startIndex = typeof pageParam === 'number' ? pageParam : 0;
      return runExit(
        fetchVideoLibraryPage(
          collectionType(),
          params().libraryId,
          startIndex,
          filterSort(),
          libraryFilters.playedFilter(),
          libraryFilters.favoritesOnly(),
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

  const browsePageQueryKey = (startIndex: number) =>
    queryKeys.libraryBrowsePage(
      sessionKey(),
      collectionType(),
      params().libraryId,
      filterSort(),
      libraryFilters.playedFilter(),
      libraryFilters.favoritesOnly(),
      libraryFilters.sortDirection(),
      startIndex,
    );

  let activeBrowseQueryKey = '';
  createEffect(() => {
    const nextBrowseQueryKey = browseQueryKey().join('\u0000');
    if (activeBrowseQueryKey && activeBrowseQueryKey !== nextBrowseQueryKey) {
      setVirtualPagesByStartIndex(new Map<number, LibraryExit<LibraryBrowseState>>());
      setVirtualPageStartsFetching(new Set<number>());
    }
    activeBrowseQueryKey = nextBrowseQueryKey;
  });

  const successfulPages = () =>
    browseQuery.data?.pages.filter(
      (page): page is LibraryExit<LibraryBrowseState> & { _tag: 'Success' } => Exit.isSuccess(page),
    ) ?? [];

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
    const isDefaultAsc = filterSort() === 'title';
    return isDefaultAsc
      ? libraryFilters.sortDirection() === 'desc'
      : libraryFilters.sortDirection() === 'asc';
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
  const columnCount = createMemo(() => libraryBrowseColumnCount(virtualGridWidth()));
  const virtualRowColumnIndexes = createMemo(() =>
    Array.from({ length: columnCount() }, (_, index) => index),
  );
  const estimateVirtualRowHeight = () => {
    const width = virtualGridWidth();
    const columns = columnCount();
    const cardWidth = Math.max(160, (width - LIBRARY_BROWSE_GRID_GAP_PX * (columns - 1)) / columns);
    return Math.ceil(cardWidth * 1.5 + 92);
  };
  const serverIndexForDisplayIndex = (displayIndex: number) =>
    needsReverse() ? totalRecordCount() - 1 - displayIndex : displayIndex;
  const pageStartForServerIndex = (serverIndex: number) =>
    Math.floor(serverIndex / LIBRARY_BROWSE_PAGE_SIZE) * LIBRARY_BROWSE_PAGE_SIZE;
  const itemForDisplayIndex = (displayIndex: number) => {
    const serverIndex = serverIndexForDisplayIndex(displayIndex);
    const pageStart = pageStartForServerIndex(serverIndex);
    const page = successfulPageMap().get(pageStart);
    return page?.items[serverIndex - page.page.startIndex] ?? null;
  };
  const loadedDisplayItemCount = () =>
    Math.min(
      totalRecordCount(),
      [...successfulPageMap().values()].reduce((count, page) => count + page.items.length, 0),
    );
  const rowVirtualizer = createVirtualizer<HTMLElement, HTMLDivElement>({
    get count() {
      return usesVirtualGrid() ? Math.ceil(totalRecordCount() / columnCount()) : 0;
    },
    getScrollElement: () => appScroll.viewport(),
    estimateSize: estimateVirtualRowHeight,
    overscan: LIBRARY_BROWSE_GRID_OVERSCAN_ROWS,
    observeElementRect: (instance, callback) =>
      observeElementRect(instance, (rect) =>
        callback({
          width: rect.width || fallbackVirtualGridWidth(),
          height: rect.height || fallbackVirtualGridHeight(),
        }),
      ),
    get initialRect() {
      return { width: fallbackVirtualGridWidth(), height: fallbackVirtualGridHeight() };
    },
    get scrollMargin() {
      return virtualScrollMargin();
    },
  });
  const browseQueryKeyMatches = (expected: readonly unknown[]) => {
    const current = browseQueryKey();
    return (
      expected.length === current.length &&
      expected.every((value, index) => value === current[index])
    );
  };
  const virtualPageStartsForCurrentWindow = () => {
    const starts = new Set<number>();
    const total = totalRecordCount();
    const columns = columnCount();

    for (const virtualRow of rowVirtualizer.getVirtualItems()) {
      for (let columnIndex = 0; columnIndex < columns; columnIndex += 1) {
        const displayIndex = virtualRow.index * columns + columnIndex;
        if (displayIndex >= total) {
          continue;
        }

        starts.add(pageStartForServerIndex(serverIndexForDisplayIndex(displayIndex)));
      }
    }

    return starts;
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

    const collectionTypeValue = collectionType();
    const libraryId = params().libraryId;
    const sort = filterSort();
    const playedFilter = libraryFilters.playedFilter();
    const favoritesOnly = libraryFilters.favoritesOnly();
    const expectedBrowseQueryKey = browseQueryKey();
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
              collectionTypeValue,
              libraryId,
              startIndex,
              sort,
              playedFilter,
              favoritesOnly,
            ),
          ),
      })
      .then((page) => {
        if (!browseQueryKeyMatches(expectedBrowseQueryKey)) {
          return;
        }

        setVirtualPagesByStartIndex((current) => new Map([...current, [startIndex, page]]));
      })
      .finally(() => {
        if (!browseQueryKeyMatches(expectedBrowseQueryKey)) {
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
    for (const startIndex of virtualPageStartsForCurrentWindow()) {
      fetchVirtualPage(startIndex, allowNetworkFetch);
    }
  };
  const canUseVirtualPages = () => {
    const currentFirstPage = firstPage();

    return (
      libraryFilters.ready() &&
      isMountedSessionActive() &&
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
  const statusTitle = () => {
    const current = firstPage();
    if (!current) {
      return `Loading ${libraryTitle(collectionType())}`;
    }
    if (Exit.isSuccess(current) && current.value.items.length === 0) {
      return `${libraryTitle(collectionType())} has no results`;
    }
    if (!Exit.isSuccess(current)) {
      return 'Could not load Library page';
    }
    return `Loading ${libraryTitle(collectionType())}`;
  };
  const statusDescription = () => {
    const current = firstPage();
    if (current && Exit.isSuccess(current) && current.value.items.length === 0) {
      return 'Jellyfin returned an empty server page for this video library.';
    }
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load Library page');
    }
    return 'JellyPilot is loading a server-paged video library result set.';
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

  createEffect(() => {
    const sentinel = autoLoadSentinel();
    if (!sentinel || typeof IntersectionObserver === 'undefined') {
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        setAutoLoadSentinelVisible(entries.some((entry) => entry.isIntersecting));
      },
      {
        root: null,
        rootMargin: '400px 0px',
        threshold: 0,
      },
    );
    observer.observe(sentinel);
    onCleanup(() => observer.disconnect());
  });

  createEffect(() => {
    if (!autoLoadSentinelVisible()) {
      return;
    }
    if (
      usesVirtualGrid() ||
      !browseQuery.hasNextPage ||
      browseQuery.isFetching ||
      laterPageFailure()
    ) {
      return;
    }
    void browseQuery.fetchNextPage({ cancelRefetch: false });
  });
  const controlsLoading = () =>
    !readyState() && (!libraryFilters.ready() || browseQuery.isFetching);

  return (
    <div class="min-w-0">
      <LibraryBrowseNavbarControls
        loading={controlsLoading}
        sortedValue={libraryFilters.sort}
        sortDirection={libraryFilters.sortDirection}
        playedFilter={libraryFilters.playedFilter}
        favoritesOnly={libraryFilters.favoritesOnly}
        onSortChange={libraryFilters.setSort}
        onSortDirectionChange={libraryFilters.setSortDirection}
        onPlayedFilterChange={libraryFilters.setPlayedFilter}
        onFavoritesOnlyChange={libraryFilters.setFavoritesOnly}
      />

      <Suspense fallback={<LibraryBrowseSkeleton />}>
        <Show
          when={readyState()}
          fallback={
            !libraryFilters.ready() || browseQuery.isPending ? (
              <LibraryBrowseSkeleton />
            ) : (
              <LibraryStatusPanel title={statusTitle()} description={statusDescription()} />
            )
          }
        >
          <section class="space-y-4" aria-labelledby="library-browse-title">
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <h2 id="library-browse-title" class="text-on-surface text-[22px] leading-7 font-bold">
                {libraryTitle(collectionType())}
              </h2>
              <p class="text-on-surface-variant/80 text-[12px] leading-4 tabular-nums">
                <Show
                  when={usesVirtualGrid()}
                  fallback={
                    <>
                      {readyState()?.items.length ?? 0} of {totalRecordCount()}
                    </>
                  }
                >
                  {loadedDisplayItemCount()} of {totalRecordCount()}
                </Show>
              </p>
            </div>
            <Show
              when={usesVirtualGrid()}
              fallback={
                <div
                  class={`${LIBRARY_BROWSE_AUTO_GRID_CLASS} animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards]`}
                >
                  <For each={readyState()?.items ?? []}>
                    {(item) => (
                      <VideoCard kind="library" item={item} collectionType={collectionType()} />
                    )}
                  </For>
                  <Show when={browseQuery.isFetchingNextPage}>
                    <LibraryBrowseSkeletonCards />
                  </Show>
                </div>
              }
            >
              <div
                ref={setVirtualGrid}
                data-testid="library-virtual-grid"
                class="animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards]"
              >
                <div
                  class="relative w-full"
                  style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
                >
                  <For each={rowVirtualizer.getVirtualItems()}>
                    {(virtualRow) => (
                      <div
                        class="absolute top-0 left-0 w-full"
                        style={{
                          height: `${virtualRow.size}px`,
                          transform: `translateY(${virtualRow.start - virtualScrollMargin()}px)`,
                        }}
                      >
                        <div class={LIBRARY_BROWSE_AUTO_GRID_CLASS}>
                          <For each={virtualRowColumnIndexes()}>
                            {(columnIndex) => {
                              const displayIndex = () =>
                                virtualRow.index * columnCount() + columnIndex;
                              const item = () => itemForDisplayIndex(displayIndex());

                              return (
                                <Show when={displayIndex() < totalRecordCount()}>
                                  <Show when={item()} fallback={<LibraryBrowseSkeletonCard />}>
                                    {(loadedItem) => (
                                      <VideoCard
                                        kind="library"
                                        item={loadedItem()}
                                        collectionType={collectionType()}
                                      />
                                    )}
                                  </Show>
                                </Show>
                              );
                            }}
                          </For>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
            <Show when={loadMoreErrorDescription()}>
              {(message) => (
                <div class="flex flex-col items-center gap-3 pt-2">
                  <p class="text-error text-center text-[12px] leading-[16px]">{message()}</p>
                  <Button
                    type="button"
                    variant="secondary"
                    class="rounded-full"
                    disabled={loadMoreRetryBusy()}
                    onClick={retryFailedPage}
                    leadingIcon={
                      <RefreshCw
                        class="h-4 w-4"
                        classList={{ 'animate-spin': loadMoreRetryBusy() }}
                      />
                    }
                  >
                    Retry loading more
                  </Button>
                </div>
              )}
            </Show>
            <div ref={setAutoLoadSentinel} aria-hidden="true" class="h-px w-full" />
          </section>
        </Show>
      </Suspense>
    </div>
  );
}
interface LibrarySortMenuProps {
  value: () => VideoLibrarySort;
  onChange: (sort: VideoLibrarySort) => void;
  disabled: () => boolean;
}

function LibrarySortMenu(props: LibrarySortMenuProps) {
  return (
    <Menu.Root>
      <Menu.Trigger
        disabled={props.disabled()}
        aria-label="Sort By"
        class="border-outline-variant text-on-surface hover:text-secondary flex h-10 w-10 flex-none items-center justify-center border-l px-2 text-left transition-colors duration-200 outline-none disabled:cursor-not-allowed disabled:opacity-50 sm:h-12 sm:w-12"
      >
        <ListSortAscending size={14} />
      </Menu.Trigger>
      <Menu.Positioner>
        <Menu.Content class="border-outline-variant bg-surface-container-lowest z-50 max-h-60 min-w-48 overflow-y-auto rounded-lg border p-2 shadow-2xl backdrop-blur-md focus:outline-none">
          <Menu.RadioItemGroup
            value={props.value()}
            onValueChange={(details) => props.onChange(details.value as VideoLibrarySort)}
          >
            <Menu.ItemGroupLabel class="px-3.5 py-2 text-xs font-bold">Sort By</Menu.ItemGroupLabel>
            <For each={sortItems}>
              {(item) => (
                <Menu.RadioItem
                  value={item.value}
                  class="text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-[14px] leading-5 transition-colors data-disabled:cursor-not-allowed data-disabled:opacity-50"
                >
                  <Menu.ItemText class="font-medium">
                    <span>{item.label}</span>
                  </Menu.ItemText>
                  <Menu.ItemIndicator>
                    <Check class="text-secondary h-4 w-4" />
                  </Menu.ItemIndicator>
                </Menu.RadioItem>
              )}
            </For>
          </Menu.RadioItemGroup>
        </Menu.Content>
      </Menu.Positioner>
    </Menu.Root>
  );
}

interface LibraryStatusMenuProps {
  value: () => VideoLibraryPlayedFilter;
  onChange: (filter: VideoLibraryPlayedFilter) => void;
  favoritesOnly: () => boolean;
  onFavoritesOnlyChange: (favoritesOnly: boolean) => void;
  disabled: () => boolean;
}

function LibraryStatusMenu(props: LibraryStatusMenuProps) {
  return (
    <Menu.Root>
      <Menu.Trigger
        disabled={props.disabled()}
        aria-label="Status"
        class="border-outline-variant text-on-surface hover:text-secondary flex h-10 w-10 flex-none items-center justify-center border-l px-2 text-left transition-colors duration-200 outline-none disabled:cursor-not-allowed disabled:opacity-50 sm:h-12 sm:w-12"
      >
        <Funnel size={14} />
      </Menu.Trigger>
      <Menu.Positioner>
        <Menu.Content class="border-outline-variant bg-surface-container-lowest z-50 max-h-60 min-w-48 overflow-y-auto rounded-lg border p-2 shadow-2xl backdrop-blur-md focus:outline-none">
          <Menu.RadioItemGroup
            value={props.value()}
            onValueChange={(details) => props.onChange(details.value as VideoLibraryPlayedFilter)}
          >
            <Menu.ItemGroupLabel class="px-3.5 py-2 text-xs font-bold">Status</Menu.ItemGroupLabel>
            <For each={['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]}>
              {(filter) => (
                <Menu.RadioItem
                  value={filter}
                  class="text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-[14px] leading-5 transition-colors data-disabled:cursor-not-allowed data-disabled:opacity-50"
                >
                  <Menu.ItemText class="font-medium">
                    <span>{playedFilterLabel(filter)}</span>
                  </Menu.ItemText>
                  <Menu.ItemIndicator>
                    <Check class="text-secondary h-4 w-4" />
                  </Menu.ItemIndicator>
                </Menu.RadioItem>
              )}
            </For>
          </Menu.RadioItemGroup>

          <div class="border-outline-variant/60 my-1 border-t" />

          <Menu.CheckboxItem
            checked={props.favoritesOnly()}
            onCheckedChange={(checked) => props.onFavoritesOnlyChange(checked)}
            value="favorites"
            class="text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-[14px] leading-5 transition-colors data-disabled:cursor-not-allowed data-disabled:opacity-50"
          >
            <Menu.ItemText class="font-medium">
              <span>Favorites Only</span>
            </Menu.ItemText>
            <Menu.ItemIndicator>
              <Check class="text-secondary h-4 w-4" />
            </Menu.ItemIndicator>
          </Menu.CheckboxItem>
        </Menu.Content>
      </Menu.Positioner>
    </Menu.Root>
  );
}

interface LibraryBrowseNavbarControlsProps {
  loading: () => boolean;
  sortedValue: () => VideoLibrarySort;
  sortDirection: () => LibrarySortDirection;
  playedFilter: () => VideoLibraryPlayedFilter;
  favoritesOnly: () => boolean;
  onSortChange: (sort: VideoLibrarySort) => void;
  onSortDirectionChange: (direction: LibrarySortDirection) => void;
  onPlayedFilterChange: (filter: VideoLibraryPlayedFilter) => void;
  onFavoritesOnlyChange: (favoritesOnly: boolean) => void;
}

function LibraryBrowseNavbarControls(props: LibraryBrowseNavbarControlsProps) {
  const navbarControls = useLibraryNavbarControls();

  return (
    <Show when={navbarControls.portalTarget()}>
      {(target) => (
        <Portal mount={target()}>
          <nav class="flex flex-row items-end justify-between" aria-label="Library browse controls">
            <div class="flex min-w-0 flex-wrap justify-end overflow-hidden rounded-2xl">
              <Toggle.Root
                pressed={props.sortDirection() === 'desc'}
                onPressedChange={(pressed) => {
                  props.onSortDirectionChange(pressed ? 'desc' : 'asc');
                }}
                disabled={props.loading()}
                aria-label={props.sortDirection() === 'desc' ? 'Sort descending' : 'Sort ascending'}
                class="border-outline-variant text-on-surface hover:text-secondary data-[state=on]:bg-secondary-container/45 data-[state=on]:text-on-secondary-container flex h-10 w-10 items-center justify-center border-l transition-colors duration-200 outline-none disabled:cursor-not-allowed disabled:opacity-50 sm:h-12 sm:w-12"
              >
                <Show
                  when={props.sortDirection() === 'desc'}
                  fallback={<ArrowUpWideNarrowIcon size={14} />}
                >
                  <ArrowDownWideNarrowIcon size={14} />
                </Show>
              </Toggle.Root>
              <LibrarySortMenu
                value={props.sortedValue}
                onChange={props.onSortChange}
                disabled={props.loading}
              />
              <LibraryStatusMenu
                value={props.playedFilter}
                onChange={props.onPlayedFilterChange}
                favoritesOnly={props.favoritesOnly}
                onFavoritesOnlyChange={props.onFavoritesOnlyChange}
                disabled={props.loading}
              />
            </div>
          </nav>
        </Portal>
      )}
    </Show>
  );
}

function LibraryBrowseSkeletonCard() {
  return <VideoCard kind="library" collectionType="movies" loading />;
}

function LibraryBrowseSkeletonCards() {
  return <For each={LIBRARY_BROWSE_SKELETON_CARD_KEYS}>{() => <LibraryBrowseSkeletonCard />}</For>;
}

function LibraryBrowseSkeleton() {
  return (
    <section class="space-y-4" aria-hidden="true">
      <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div class="bg-surface-container-high/70 h-7 w-32 animate-pulse rounded-md" />
        <div class="bg-surface-container-high/60 h-4 w-24 animate-pulse rounded" />
      </div>
      <div class={LIBRARY_BROWSE_AUTO_GRID_CLASS}>
        <LibraryBrowseSkeletonCards />
      </div>
    </section>
  );
}
