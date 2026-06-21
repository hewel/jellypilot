import { Menu } from '@ark-ui/solid/menu';
import { Toggle } from '@ark-ui/solid/toggle';
import type { VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibrarySort } from '@bindings';
import { useLibraryNavbarControls } from '@components/library/LibraryNavbarContext';
import {
  LibraryStatusPanel,
  MediaInfoHoverCard,
  VideoLibraryCard,
  libraryTitle,
  playedFilterLabel,
  sortItems,
} from '@components/library/shared';
import { Button, Card } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import {
  Check,
  RefreshCw,
  ListSortAscending,
  Funnel,
  ArrowDownWideNarrowIcon,
  ArrowUpWideNarrowIcon,
} from 'lucide-solid';
import { For, Show, Suspense, createResource, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';
import { commandFailureMessage } from '~effects/commands';
import { fetchVideoLibraryPage } from '~effects/library';
import type { LibraryBrowseState, LibraryExit } from '~effects/library';

const INITIAL_SORT: VideoLibrarySort = 'title';
const INITIAL_PLAYED_FILTER: VideoLibraryPlayedFilter = 'all';
const INITIAL_FAVORITES_ONLY = false;

function collectionTypeFromParam(collectionType: string): VideoLibraryKind {
  return collectionType === 'tvshows' ? 'tvshows' : 'movies';
}

export const Route = createFileRoute('/_authenticated/library/$collectionType/$libraryId')({
  loader: ({ params }) => ({
    initialPage: fetchVideoLibraryPage(
      collectionTypeFromParam(params.collectionType),
      params.libraryId,
      0,
      INITIAL_SORT,
      INITIAL_PLAYED_FILTER,
      INITIAL_FAVORITES_ONLY,
    ),
  }),
  component: LibraryBrowseRoute,
});

function LibraryBrowseRoute() {
  const params = Route.useParams();
  const loaderData = Route.useLoaderData();
  const [initialPage] = createResource(() => loaderData().initialPage);
  const [state, setState] = createSignal<LibraryExit<LibraryBrowseState> | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [usingLoaderPage, setUsingLoaderPage] = createSignal(true);
  const [sort, setSort] = createSignal<VideoLibrarySort>(INITIAL_SORT);
  const [playedFilter, setPlayedFilter] =
    createSignal<VideoLibraryPlayedFilter>(INITIAL_PLAYED_FILTER);
  const [favoritesOnly, setFavoritesOnly] = createSignal(INITIAL_FAVORITES_ONLY);
  const [sortDirection, setSortDirection] = createSignal<'asc' | 'desc'>('asc');

  const collectionType = () => collectionTypeFromParam(params().collectionType);
  const currentState = () => (usingLoaderPage() ? (initialPage() ?? null) : state());

  const loadPage = async (startIndex: number, replace = false) => {
    if (loading()) {
      return;
    }
    setLoading(true);
    const previous = currentState();
    const result = await fetchVideoLibraryPage(
      collectionType(),
      params().libraryId,
      startIndex,
      sort(),
      playedFilter(),
      favoritesOnly(),
    );
    setState((current) => {
      const base = current ?? previous;
      if (!replace && base && Exit.isSuccess(base) && Exit.isSuccess(result)) {
        return Exit.succeed({
          items: [...base.value.items, ...result.value.items],
          page: result.value.page,
        });
      }
      return result;
    });
    setUsingLoaderPage(false);
    setLoading(false);
  };
  const reloadFromFirstPage = () => {
    setUsingLoaderPage(false);
    setState(null);
    void loadPage(0, true);
  };

  const readyState = () => {
    const current = currentState();
    if (!current || !Exit.isSuccess(current)) {
      return null;
    }
    const val = current.value;
    const isDefaultAsc = sort() === 'title';
    const needsReverse = isDefaultAsc ? sortDirection() === 'desc' : sortDirection() === 'asc';

    if (needsReverse) {
      // Ponytail: backend lacks direction param, reverse current items array for opposite order
      // eslint-disable-next-line unicorn/no-array-reverse
      const reversed = [...val.items].reverse();
      return {
        ...val,
        items: reversed,
      };
    }
    return val;
  };
  const statusTitle = () => {
    const current = currentState();
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
    const current = currentState();
    if (current && Exit.isSuccess(current) && current.value.items.length === 0) {
      return 'Jellyfin returned an empty server page for this video library.';
    }
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load Library page');
    }
    return 'JMSR is loading a server-paged video library result set.';
  };
  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  return (
    <div class="min-w-0">
      <LibraryBrowseNavbarControls
        loading={loading}
        sortedValue={sort}
        sortDirection={sortDirection}
        playedFilter={playedFilter}
        favoritesOnly={favoritesOnly}
        onSortChange={(value) => {
          setSort(value);
          reloadFromFirstPage();
        }}
        onSortDirectionChange={(direction) => {
          setSortDirection(direction);
          reloadFromFirstPage();
        }}
        onPlayedFilterChange={(filter) => {
          setPlayedFilter(filter);
          reloadFromFirstPage();
        }}
        onFavoritesOnlyChange={(value) => {
          setFavoritesOnly(value);
          reloadFromFirstPage();
        }}
      />

      <Suspense fallback={<LibraryBrowseSkeleton />}>
        <Show
          when={readyState()}
          fallback={
            loading() ? (
              <LibraryBrowseSkeleton />
            ) : (
              <LibraryStatusPanel title={statusTitle()} description={statusDescription()} />
            )
          }
        >
          <section class="space-y-4" aria-labelledby="library-browse-title">
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <h2
                id="library-browse-title"
                class="text-on-surface text-[22px] leading-[28px] font-bold"
              >
                {libraryTitle(collectionType())}
              </h2>
              <p class="text-on-surface-variant/80 text-[12px] leading-[16px]">
                {readyState()?.items.length ?? 0} of {readyState()?.page.totalRecordCount ?? 0}
              </p>
            </div>
            <div class="grid animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
              <For each={readyState()?.items ?? []}>
                {(item) => (
                  <MediaInfoHoverCard id={item.id} itemType={item.itemType}>
                    <VideoLibraryCard item={item} collectionType={collectionType()} />
                  </MediaInfoHoverCard>
                )}
              </For>
            </div>
            <Show when={readyState()?.page.hasMore}>
              <div class="flex justify-center pt-2">
                <Button
                  type="button"
                  variant="secondary"
                  class="rounded-full"
                  disabled={loading()}
                  onClick={() => void loadPage(loadMoreStartIndex())}
                  leadingIcon={
                    <RefreshCw class="h-4 w-4" classList={{ 'animate-spin': loading() }} />
                  }
                >
                  {loading() ? 'Loading more' : 'Load more'}
                </Button>
              </div>
            </Show>
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
        class="border-outline-variant text-on-surface hover:text-secondary flex h-12 w-full flex-1 items-center justify-between border-l px-3 text-left transition-all duration-200 outline-none disabled:cursor-not-allowed disabled:opacity-50"
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
        class="border-outline-variant text-on-surface hover:text-secondary flex h-12 w-full flex-1 items-center justify-between border-l px-3 text-left transition-all duration-200 outline-none disabled:cursor-not-allowed disabled:opacity-50"
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
  sortDirection: () => 'asc' | 'desc';
  playedFilter: () => VideoLibraryPlayedFilter;
  favoritesOnly: () => boolean;
  onSortChange: (sort: VideoLibrarySort) => void;
  onSortDirectionChange: (direction: 'asc' | 'desc') => void;
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
            <div class="flex min-w-0">
              <Toggle.Root
                pressed={props.sortDirection() === 'desc'}
                onPressedChange={(pressed) => {
                  props.onSortDirectionChange(pressed ? 'desc' : 'asc');
                }}
                disabled={props.loading()}
                aria-label={props.sortDirection() === 'desc' ? 'Sort descending' : 'Sort ascending'}
                class="border-outline-variant text-on-surface hover:text-secondary data-[state=on]:bg-secondary-container/45 data-[state=on]:text-on-secondary-container flex h-12 w-12 items-center justify-center border-l transition-all duration-200 outline-none disabled:cursor-not-allowed disabled:opacity-50"
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

function LibraryBrowseSkeleton() {
  return (
    <section class="space-y-4" aria-hidden="true">
      <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div class="bg-surface-container-high/70 h-7 w-32 animate-pulse rounded-md" />
        <div class="bg-surface-container-high/60 h-4 w-24 animate-pulse rounded" />
      </div>
      <div class="grid gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
        <For each={[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]}>
          {() => (
            <Card variant="filled" surfaceTint={false} class="overflow-hidden !p-0">
              <div class="border-outline-variant bg-surface-container-lowest/60 aspect-[2/3] animate-pulse border-b" />
              <div class="space-y-2 p-4">
                <div class="bg-surface-container-high/80 h-4 w-4/5 animate-pulse rounded" />
                <div class="bg-surface-container-high/60 h-3 w-3/5 animate-pulse rounded" />
                <div class="bg-surface-container-high/50 h-3 w-1/3 animate-pulse rounded" />
              </div>
            </Card>
          )}
        </For>
      </div>
    </section>
  );
}
