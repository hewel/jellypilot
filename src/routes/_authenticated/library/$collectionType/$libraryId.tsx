import { Menu } from '@ark-ui/solid/menu';
import { Toggle } from '@ark-ui/solid/toggle';
import type { VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibrarySort } from '@bindings';
import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { useAuthenticatedBootstrap } from '@components/AuthenticatedBootstrap';
import { LibraryVideoCard } from '@components/library/LibraryVideoCard';
import {
  LibraryStatusPanel,
  libraryTitle,
  playedFilterLabel,
  sortItems,
} from '@components/library/shared';
import LibrarySearchBar from '@components/LibrarySearchBar';
import { Button } from '@components/ui';
import { cx } from '@styled-system/css';
import { createInfiniteQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, redirect } from '@tanstack/solid-router';
import { createVirtualizer, observeElementRect } from '@tanstack/solid-virtual';
import { Exit } from 'effect';
import {
  ArrowDownWideNarrowIcon,
  ArrowUpWideNarrowIcon,
  Check,
  ChevronDown,
  Funnel,
  Heart,
  ListSortAscending,
  RefreshCw,
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
import { commandFailureMessage } from '~effects/commands';
import { LIBRARY_BROWSE_PAGE_SIZE, fetchVideoLibraryPage } from '~effects/library';
import type { LibraryBrowseState, LibraryExit } from '~effects/library';
import { librarySessionSignature, queryKeys, runExit } from '~effects/query';
import type { LibrarySessionKey } from '~effects/query';
import * as recipes from '~styles/recipes';
import { createSharedLibraryFilters } from '~utils/createSharedLibraryFilters';
import type { LibrarySortDirection } from '~utils/createSharedLibraryFilters';
import {
  libraryBrowseColumnCount,
  libraryBrowseVirtualOverscanRows,
  libraryBrowseVirtualRowHeight,
} from '~utils/libraryBrowseLayout';
import {
  libraryBrowsePageLocationForDisplayIndex,
  libraryBrowsePageStartsForRows,
} from '~utils/libraryBrowsePageSelection';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../browseRoute.styles';

const LIBRARY_BROWSE_SKELETON_CARD_KEYS = Array.from({ length: 10 }, (_, index) => index);
const LIBRARY_VIRTUAL_TOTAL_THRESHOLD = 100;

interface LibraryBrowseInfiniteData {
  pages: LibraryExit<LibraryBrowseState>[];
  pageParams: number[];
}

function collectionTypeFromParam(collectionType: string): VideoLibraryKind {
  return collectionType === 'tvshows' ? 'tvshows' : 'movies';
}

export const Route = createFileRoute('/_authenticated/library/$collectionType/$libraryId')({
  beforeLoad: ({ params }) => {
    if (params.collectionType !== 'movies' && params.collectionType !== 'tvshows') {
      throw redirect({ to: AUTHENTICATED_HOME_ROUTE });
    }
  },
  component: LibraryBrowseRoute,
});

function LibraryBrowseRoute() {
  const params = Route.useParams();
  const queryClient = useQueryClient();
  const libraryFilters = createSharedLibraryFilters();
  const filterSort = libraryFilters.sort;
  const [autoLoadSentinel, setAutoLoadSentinel] = createSignal<HTMLDivElement | null>(null);
  const [autoLoadSentinelVisible, setAutoLoadSentinelVisible] = createSignal(false);
  const [virtualGrid, setVirtualGrid] = createSignal<HTMLDivElement | null>(null);
  const [virtualGridWidth, setVirtualGridWidth] = createSignal(1280);
  const [virtualViewportHeight, setVirtualViewportHeight] = createSignal(720);
  const [virtualizerMounted, setVirtualizerMounted] = createSignal(false);
  const appScroll = useAppScrollArea();
  const [virtualScrollMargin, setVirtualScrollMargin] = createSignal(0);
  const [virtualPagesByStartIndex, setVirtualPagesByStartIndex] = createSignal(
    new Map<number, LibraryExit<LibraryBrowseState>>(),
  );
  const [virtualPageStartsFetching, setVirtualPageStartsFetching] = createSignal(new Set<number>());
  onMount(() => setVirtualizerMounted(true));
  const bootstrap = useAuthenticatedBootstrap();
  const sessionKey = bootstrap.sessionKey;
  const activeSessionSignature = createMemo(() => librarySessionSignature(sessionKey()));
  let mountedSessionSignature: string | null = null;
  const isMountedSessionActive = () => {
    const currentSessionSignature = activeSessionSignature();
    return (
      currentSessionSignature !== null &&
      (mountedSessionSignature === null || mountedSessionSignature === currentSessionSignature)
    );
  };

  createEffect(() => {
    const currentSessionSignature = activeSessionSignature();
    if (currentSessionSignature !== null && mountedSessionSignature === null) {
      mountedSessionSignature = currentSessionSignature;
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
  const browseQuerySignature = createMemo(() => JSON.stringify(browseQueryKey()));
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

  let activeBrowseQuerySignature = '';
  createEffect(() => {
    const nextSignature = browseQuerySignature();
    if (activeBrowseQuerySignature && activeBrowseQuerySignature !== nextSignature) {
      setVirtualPagesByStartIndex(new Map<number, LibraryExit<LibraryBrowseState>>());
      setVirtualPageStartsFetching(new Set<number>());
      appScroll.scrollTo({ top: 0 });
    }
    activeBrowseQuerySignature = nextSignature;
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
  const estimateVirtualRowHeight = () => libraryBrowseVirtualRowHeight(virtualGridWidth());
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

  const toolbarCount = () => {
    const state = readyState();
    if (!state) {
      return null;
    }
    const loaded = usesVirtualGrid() ? loadedDisplayItemCount() : state.items.length;
    return `${loaded} of ${totalRecordCount()}`;
  };

  return (
    <div class={styles.root}>
      <LibraryBrowseToolbar
        title={() => libraryTitle(collectionType())}
        count={toolbarCount}
        loading={controlsLoading}
        sessionKey={sessionKey}
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
          <section class={styles.section} aria-label={libraryTitle(collectionType())}>
            <Show
              when={usesVirtualGrid()}
              fallback={
                <div class={cx(styles.grid, styles.fade)}>
                  <For each={readyState()?.items ?? []}>
                    {(item) => <LibraryVideoCard item={item} collectionType={collectionType()} />}
                  </For>
                  <Show when={browseQuery.isFetchingNextPage}>
                    <LibraryBrowseSkeletonCards />
                  </Show>
                </div>
              }
            >
              <div ref={setVirtualGrid} data-testid="library-virtual-grid">
                <div
                  aria-label={`${libraryTitle(collectionType())} library items`}
                  aria-rowcount={Math.ceil(totalRecordCount() / columnCount())}
                  class={styles.virtualCanvas}
                  role="grid"
                  style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
                >
                  <For each={rowVirtualizer.getVirtualItems()}>
                    {(virtualRow) => (
                      <div
                        aria-rowindex={virtualRow.index + 1}
                        class={styles.virtualRow}
                        role="row"
                        style={{
                          height: `${virtualRow.size}px`,
                          transform: `translateY(${virtualRow.start - virtualScrollMargin()}px)`,
                        }}
                      >
                        <div class={styles.grid} role="presentation">
                          <For each={virtualRowColumnIndexes()}>
                            {(columnIndex) => {
                              const displayIndex = () =>
                                virtualRow.index * columnCount() + columnIndex;
                              const item = () => itemForDisplayIndex(displayIndex());

                              return (
                                <Show when={displayIndex() < totalRecordCount()}>
                                  <div role="gridcell">
                                    <Show when={item()} fallback={<LibraryBrowseSkeletonCard />}>
                                      {(loadedItem) => (
                                        <LibraryVideoCard
                                          item={loadedItem()}
                                          collectionType={collectionType()}
                                        />
                                      )}
                                    </Show>
                                  </div>
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
                <div class={styles.loadMoreError}>
                  <p class={styles.error}>{message()}</p>
                  <Button
                    type="button"
                    variant="secondary"
                    class={recipes.pillButton}
                    disabled={loadMoreRetryBusy()}
                    onClick={retryFailedPage}
                    leadingIcon={
                      <RefreshCw
                        class={styles.icon4}
                        classList={{ [styles.spin]: loadMoreRetryBusy() }}
                      />
                    }
                  >
                    Retry loading more
                  </Button>
                </div>
              )}
            </Show>
            <div ref={setAutoLoadSentinel} aria-hidden="true" class={styles.sentinel} />
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
  const [open, setOpen] = createSignal(false);
  const currentLabel = () => sortItems.find((item) => item.value === props.value())?.label ?? '';

  return (
    <Menu.Root onOpenChange={(details) => setOpen(details.open)}>
      <Menu.Trigger disabled={props.disabled()} aria-label="Sort By" class={styles.sortTrigger}>
        <ListSortAscending size={16} class={styles.sortTriggerIcon} />
        <span class={styles.sortTriggerText}>
          <span class={styles.sortSizer} aria-hidden="true">
            {sortItems.map((item) => item.label).join('\n')}
          </span>
          <span class={styles.sortValue}>{currentLabel()}</span>
        </span>
        <ChevronDown
          size={14}
          class={styles.chevron}
          classList={{ [styles.chevronOpen]: open() }}
        />
      </Menu.Trigger>
      <Menu.Positioner>
        <Menu.Content class={styles.menuContent}>
          <Menu.RadioItemGroup
            value={props.value()}
            onValueChange={(details) => props.onChange(details.value as VideoLibrarySort)}
          >
            <Menu.ItemGroupLabel class={styles.menuLabel}>Sort By</Menu.ItemGroupLabel>
            <For each={sortItems}>
              {(item) => (
                <Menu.RadioItem value={item.value} class={styles.menuItem}>
                  <Menu.ItemText class={styles.menuText}>
                    <span>{item.label}</span>
                  </Menu.ItemText>
                  <Menu.ItemIndicator>
                    <Check class={styles.menuCheck} />
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
  const activeFilterCount = () =>
    (props.value() === 'all' ? 0 : 1) + (props.favoritesOnly() ? 1 : 0);

  return (
    <Menu.Root>
      <Menu.Trigger
        disabled={props.disabled()}
        aria-label={
          activeFilterCount() === 0
            ? 'Status'
            : `Status, ${activeFilterCount()} ${activeFilterCount() === 1 ? 'filter' : 'filters'} active`
        }
        class={styles.statusTrigger}
        classList={{ [styles.statusTriggerActive]: activeFilterCount() > 0 }}
      >
        <Funnel size={14} class={styles.statusTriggerIcon} />
        <span class={styles.statusTriggerText}>Status</span>
        <Show when={activeFilterCount() > 0}>
          <span class={styles.statusBadge} aria-hidden="true">
            {activeFilterCount()}
          </span>
        </Show>
      </Menu.Trigger>
      <Menu.Positioner>
        <Menu.Content class={styles.menuContent}>
          <Menu.RadioItemGroup
            value={props.value()}
            onValueChange={(details) => props.onChange(details.value as VideoLibraryPlayedFilter)}
          >
            <Menu.ItemGroupLabel class={styles.menuLabel}>Status</Menu.ItemGroupLabel>
            <For each={['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]}>
              {(filter) => (
                <Menu.RadioItem value={filter} class={styles.menuItem}>
                  <Menu.ItemText class={styles.menuText}>
                    <span>{playedFilterLabel(filter)}</span>
                  </Menu.ItemText>
                  <Menu.ItemIndicator>
                    <Check class={styles.menuCheck} />
                  </Menu.ItemIndicator>
                </Menu.RadioItem>
              )}
            </For>
          </Menu.RadioItemGroup>

          <div class={styles.separator} />

          <Menu.CheckboxItem
            checked={props.favoritesOnly()}
            onCheckedChange={(checked) => props.onFavoritesOnlyChange(checked)}
            value="favorites"
            class={styles.menuItem}
          >
            <Menu.ItemText class={styles.menuText}>
              <span class={styles.menuItemRow}>
                <Heart size={14} class={styles.menuItemIcon} />
                Favorites Only
              </span>
            </Menu.ItemText>
            <Menu.ItemIndicator>
              <Check class={styles.menuCheck} />
            </Menu.ItemIndicator>
          </Menu.CheckboxItem>
        </Menu.Content>
      </Menu.Positioner>
    </Menu.Root>
  );
}

interface LibraryBrowseToolbarProps {
  title: () => string;
  count: () => string | null;
  loading: () => boolean;
  sessionKey: () => LibrarySessionKey;
  sortedValue: () => VideoLibrarySort;
  sortDirection: () => LibrarySortDirection;
  playedFilter: () => VideoLibraryPlayedFilter;
  favoritesOnly: () => boolean;
  onSortChange: (sort: VideoLibrarySort) => void;
  onSortDirectionChange: (direction: LibrarySortDirection) => void;
  onPlayedFilterChange: (filter: VideoLibraryPlayedFilter) => void;
  onFavoritesOnlyChange: (favoritesOnly: boolean) => void;
}

function LibraryBrowseToolbar(props: LibraryBrowseToolbarProps) {
  const appScroll = useAppScrollArea();
  const pinned = () => appScroll.snapshot().scrollTop > 4;

  return (
    <nav class={styles.toolbar} aria-label="Library browse controls" data-toolbar="">
      <div
        class={styles.toolbarChrome}
        data-pinned={pinned() ? '' : undefined}
        aria-hidden="true"
      />
      <div class={styles.toolbarHeadingGroup}>
        <h2 id="library-browse-title" class={styles.toolbarTitle}>
          {props.title()}
        </h2>
        <Show when={props.count()}>{(count) => <p class={styles.toolbarCount}>{count()}</p>}</Show>
      </div>
      <div class={styles.toolbarSearch}>
        <LibrarySearchBar sessionKey={props.sessionKey()} />
      </div>
      <div class={styles.controlCapsule} data-disabled={props.loading() ? '' : undefined}>
        <Toggle.Root
          pressed={props.sortDirection() === 'desc'}
          onPressedChange={(pressed) => {
            props.onSortDirectionChange(pressed ? 'desc' : 'asc');
          }}
          disabled={props.loading()}
          aria-label={props.sortDirection() === 'desc' ? 'Sort descending' : 'Sort ascending'}
          class={styles.directionToggle}
        >
          <Show
            when={props.sortDirection() === 'desc'}
            fallback={<ArrowUpWideNarrowIcon size={16} />}
          >
            <ArrowDownWideNarrowIcon size={16} />
          </Show>
        </Toggle.Root>
        <div class={styles.controlDivider} aria-hidden="true" />
        <LibrarySortMenu
          value={props.sortedValue}
          onChange={props.onSortChange}
          disabled={props.loading}
        />
      </div>
      <LibraryStatusMenu
        value={props.playedFilter}
        onChange={props.onPlayedFilterChange}
        favoritesOnly={props.favoritesOnly}
        onFavoritesOnlyChange={props.onFavoritesOnlyChange}
        disabled={props.loading}
      />
    </nav>
  );
}

function LibraryBrowseSkeletonCard() {
  return <LibraryVideoCard collectionType="movies" loading />;
}

function LibraryBrowseSkeletonCards() {
  return <For each={LIBRARY_BROWSE_SKELETON_CARD_KEYS}>{() => <LibraryBrowseSkeletonCard />}</For>;
}

function LibraryBrowseSkeleton() {
  return (
    <section class={styles.section} aria-hidden="true">
      <div class={styles.grid}>
        <LibraryBrowseSkeletonCards />
      </div>
    </section>
  );
}
