import { Menu } from '@ark-ui/solid/menu';
import { Toggle } from '@ark-ui/solid/toggle';
import type {
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '@bindings';
import { useAppScrollArea } from '@components/AppScrollAreaContext';
import { useAuthenticatedBootstrap } from '@components/AuthenticatedBootstrap';
import {
  LibraryStatusPanel,
  libraryTitle,
  playedFilterLabel,
  sortItems,
} from '@components/library/shared';
import { VideoCard } from '@components/library/VideoCard';
import { videoCardSubtitle } from '@components/library/videoCardModel';
import { VideoCardSkeleton, type VideoCardAspectClass } from '@components/library/videoCardShared';
import { Button } from '@components/ui';
import { cx } from '@styled-system/css';
import { createFileRoute, redirect } from '@tanstack/solid-router';
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
import { For, Show, Suspense, createEffect, createMemo, createSignal, onCleanup } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { librarySessionSignature } from '~effects/query';
import * as recipes from '~styles/recipes';
import { createLibraryBrowseWindow } from '~utils/createLibraryBrowseWindow';
import { createSharedLibraryFilters } from '~utils/createSharedLibraryFilters';
import type { LibrarySortDirection } from '~utils/createSharedLibraryFilters';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../browseRoute.styles';

const LIBRARY_BROWSE_SKELETON_CARD_KEYS = Array.from({ length: 10 }, (_, index) => index);

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
  const libraryFilters = createSharedLibraryFilters();
  const [autoLoadSentinel, setAutoLoadSentinel] = createSignal<HTMLDivElement | null>(null);
  const [autoLoadSentinelVisible, setAutoLoadSentinelVisible] = createSignal(false);
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

  const collectionType = () => collectionTypeFromParam(params().collectionType);
  const filterSort = libraryFilters.sort;
  const browseWindow = createLibraryBrowseWindow({
    request: () => ({
      sessionKey: sessionKey(),
      collectionType: collectionType(),
      libraryId: params().libraryId,
      sort: filterSort(),
      playedFilter: libraryFilters.playedFilter(),
      favoritesOnly: libraryFilters.favoritesOnly(),
      sortDirection: libraryFilters.sortDirection(),
    }),
    filtersReady: libraryFilters.ready,
    sessionActive: isMountedSessionActive,
  });

  const statusTitle = () => {
    const current = browseWindow.firstPage();
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
    const current = browseWindow.firstPage();
    if (current && Exit.isSuccess(current) && current.value.items.length === 0) {
      return 'Jellyfin returned an empty server page for this video library.';
    }
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load Library page');
    }
    return 'JellyPilot is loading a server-paged video library result set.';
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
      browseWindow.usesVirtualGrid() ||
      !browseWindow.hasNextPage() ||
      browseWindow.isFetching() ||
      browseWindow.laterPageFailure()
    ) {
      return;
    }
    browseWindow.fetchNextPage();
  });
  const controlsLoading = () =>
    !browseWindow.readyState() && (!libraryFilters.ready() || browseWindow.isFetching());

  const toolbarCount = () => {
    const state = browseWindow.readyState();
    if (!state) {
      return null;
    }
    const loaded = browseWindow.usesVirtualGrid()
      ? browseWindow.loadedDisplayItemCount()
      : state.items.length;
    return `${loaded} of ${browseWindow.totalRecordCount()}`;
  };

  return (
    <div class={styles.root}>
      <LibraryBrowseToolbar
        title={() => libraryTitle(collectionType())}
        count={toolbarCount}
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
          when={browseWindow.readyState()}
          fallback={
            !libraryFilters.ready() || browseWindow.isPending() ? (
              <LibraryBrowseSkeleton />
            ) : (
              <LibraryStatusPanel title={statusTitle()} description={statusDescription()} />
            )
          }
        >
          <section class={styles.section} aria-label={libraryTitle(collectionType())}>
            <Show
              when={browseWindow.usesVirtualGrid()}
              fallback={
                <div class={cx(styles.grid, styles.fade)}>
                  <For each={browseWindow.readyState()?.items ?? []}>
                    {(item) => <LibraryBrowseCard item={item} collectionType={collectionType()} />}
                  </For>
                  <Show when={browseWindow.isFetchingNextPage()}>
                    <LibraryBrowseSkeletonCards />
                  </Show>
                </div>
              }
            >
              <div ref={browseWindow.virtual.ref} data-testid="library-virtual-grid">
                <div
                  aria-label={`${libraryTitle(collectionType())} library items`}
                  aria-rowcount={Math.ceil(
                    browseWindow.totalRecordCount() / browseWindow.virtual.columnCount(),
                  )}
                  class={styles.virtualCanvas}
                  role="grid"
                  style={{ height: `${browseWindow.virtual.totalSize()}px` }}
                >
                  <For each={browseWindow.virtual.items()}>
                    {(virtualRow) => (
                      <div
                        aria-rowindex={virtualRow.index + 1}
                        class={styles.virtualRow}
                        role="row"
                        style={{
                          height: `${virtualRow.size}px`,
                          transform: `translateY(${virtualRow.start - browseWindow.virtual.scrollMargin()}px)`,
                        }}
                      >
                        <div class={styles.grid} role="presentation">
                          <For each={browseWindow.virtual.rowColumnIndexes()}>
                            {(columnIndex) => {
                              const displayIndex = () =>
                                virtualRow.index * browseWindow.virtual.columnCount() + columnIndex;
                              const item = () => browseWindow.itemForDisplayIndex(displayIndex());

                              return (
                                <Show when={displayIndex() < browseWindow.totalRecordCount()}>
                                  <div role="gridcell">
                                    <Show when={item()} fallback={<LibraryBrowseSkeletonCard />}>
                                      {(loadedItem) => (
                                        <LibraryBrowseCard
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
            <Show when={browseWindow.loadMoreErrorDescription()}>
              {(message) => (
                <div class={styles.loadMoreError}>
                  <p class={styles.error}>{message()}</p>
                  <Button
                    type="button"
                    variant="secondary"
                    class={recipes.pillButton}
                    disabled={browseWindow.loadMoreRetryBusy()}
                    onClick={browseWindow.retryFailedPage}
                    leadingIcon={
                      <RefreshCw
                        class={styles.icon4}
                        classList={{ [styles.spin]: browseWindow.loadMoreRetryBusy() }}
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
  const currentLabel = () => sortItems.find((item) => item.value === props.value())?.label ?? '';

  return (
    <Menu.Root>
      <Menu.Trigger disabled={props.disabled()} aria-label="Sort By" class={styles.sortTrigger}>
        <ListSortAscending size={16} class={styles.sortTriggerIcon} />
        <span class={styles.sortTriggerText}>
          <span class={styles.sortSizer} aria-hidden="true">
            {sortItems.map((item) => item.label).join('\n')}
          </span>
          <span class={styles.sortValue}>{currentLabel()}</span>
        </span>
        <ChevronDown size={14} class={styles.chevron} />
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
        class={styles.statusTrigger({ active: activeFilterCount() > 0 })}
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

function libraryBrowseCardAspect(
  item: VideoLibraryItem,
  collectionType: VideoLibraryKind,
): VideoCardAspectClass {
  return collectionType === 'tvshows' || item.itemType === 'Series' || item.itemType === 'Movie'
    ? 'poster'
    : 'video';
}

function LibraryBrowseCard(props: { item: VideoLibraryItem; collectionType: VideoLibraryKind }) {
  const aspect = () => libraryBrowseCardAspect(props.item, props.collectionType);
  return (
    <VideoCard
      item={props.item}
      aspect={aspect()}
      copy={aspect() === 'poster' ? 'overlay' : 'below'}
      action={{ kind: 'open' }}
      subtitle={videoCardSubtitle(props.item, { kind: 'browse' })}
      badges={{ favorite: true, played: true }}
    />
  );
}

function LibraryBrowseSkeletonCard() {
  return <VideoCardSkeleton aspectClass="poster" />;
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
