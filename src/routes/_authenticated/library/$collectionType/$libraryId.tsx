import { Checkbox } from '@ark-ui/solid/checkbox';
import type {
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '@bindings';
import {
  LibraryStatusPanel,
  libraryTitle,
  playedFilterLabel,
  sortItems,
  VideoLibraryCard,
} from '@components/library/shared';
import { Button, JmsrSelect } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Check, Library, RefreshCw } from 'lucide-solid';
import { createSignal, For, onMount, Show } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import {
  fetchVideoLibraryPage,
  type LibraryBrowseState,
  type LibraryExit,
} from '~effects/library';

export const Route = createFileRoute(
  '/_authenticated/library/$collectionType/$libraryId',
)({
  component: LibraryBrowseRoute,
});

function LibraryBrowseRoute() {
  const params = Route.useParams();
  const [state, setState] =
    createSignal<LibraryExit<LibraryBrowseState> | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [sort, setSort] = createSignal<VideoLibrarySort>('title');
  const [playedFilter, setPlayedFilter] =
    createSignal<VideoLibraryPlayedFilter>('all');
  const [favoritesOnly, setFavoritesOnly] = createSignal(false);

  const collectionType: VideoLibraryKind =
    params().collectionType === 'tvshows' ? 'tvshows' : 'movies';

  const loadPage = async (startIndex: number, replace = false) => {
    if (loading()) return;
    setLoading(true);
    const result = await fetchVideoLibraryPage(
      collectionType,
      params().libraryId,
      startIndex,
      sort(),
      playedFilter(),
      favoritesOnly(),
    );
    setState((current) => {
      if (
        !replace &&
        current &&
        Exit.isSuccess(current) &&
        Exit.isSuccess(result)
      ) {
        return Exit.succeed({
          page: result.value.page,
          items: [...current.value.items, ...result.value.items],
        });
      }
      return result;
    });
    setLoading(false);
  };
  const reloadFromFirstPage = () => {
    setState(null);
    void loadPage(0, true);
  };

  onMount(() => {
    void loadPage(0, true);
  });

  const readyState = () => {
    const current = state();
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const statusTitle = () => {
    const current = state();
    if (!current) return `Loading ${libraryTitle(collectionType)}`;
    if (Exit.isSuccess(current) && current.value.items.length === 0) {
      return `${libraryTitle(collectionType)} has no results`;
    }
    if (!Exit.isSuccess(current)) return 'Could not load Library page';
    return `Loading ${libraryTitle(collectionType)}`;
  };
  const statusDescription = () => {
    const current = state();
    if (
      current &&
      Exit.isSuccess(current) &&
      current.value.items.length === 0
    ) {
      return 'Jellyfin returned an empty server page for this video library.';
    }
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(
        current.cause,
        'Could not load Library page',
      );
    }
    return 'JMSR is loading a server-paged video library result set.';
  };
  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  return (
    <div class="space-y-6">
      <header class="flex flex-col gap-4 rounded-2xl border border-outline-variant bg-surface-container-low/60 p-3 shadow-xl backdrop-blur-md lg:p-4">
        <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div class="flex items-center gap-3">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary-container/30 text-primary">
              <Library class="h-5 w-5" />
            </div>
            <div>
              <h1 class="text-title-medium font-bold text-on-surface">
                {libraryTitle(collectionType)}
              </h1>
              <p class="text-[10px] font-semibold text-on-surface-variant/80">
                Library Browser
              </p>
            </div>
          </div>

          <Button
            href="/library"
            variant="outlined"
            size="sm"
            class="h-10 rounded-xl px-4 shrink-0"
            leadingIcon={<Library class="h-4 w-4" />}
          >
            Video Home
          </Button>
        </div>

        <nav
          class="flex flex-col gap-3 xl:flex-row xl:items-end xl:justify-between"
          aria-label="Library browse controls"
        >
          <div class="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-end">
            <JmsrSelect
              label="Sort By"
              items={sortItems}
              disabled={loading()}
              value={sort()}
              placeholder="Select sort..."
              size="compact"
              onValueChange={(value) => {
                setSort(value);
                reloadFromFirstPage();
              }}
              class="min-w-[12rem] flex-1 sm:max-w-[13rem]"
            />

            <fieldset class="min-w-0 space-y-2" aria-label="Played filter">
              <legend class="text-label-medium text-on-surface-variant">
                Status
              </legend>
              <div class="flex flex-wrap gap-2">
                <For
                  each={
                    ['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]
                  }
                >
                  {(filter) => (
                    <Button
                      type="button"
                      variant="outlined"
                      size="sm"
                      class={`h-10 rounded-xl px-4 ${
                        playedFilter() === filter
                          ? 'border-secondary bg-secondary-container/45 text-on-secondary-container'
                          : ''
                      }`}
                      aria-pressed={playedFilter() === filter}
                      disabled={loading()}
                      onClick={() => {
                        setPlayedFilter(filter);
                        reloadFromFirstPage();
                      }}
                    >
                      {playedFilterLabel(filter)}
                    </Button>
                  )}
                </For>
              </div>
            </fieldset>
          </div>

          <Checkbox.Root
            checked={favoritesOnly()}
            disabled={loading()}
            onCheckedChange={(details) => {
              setFavoritesOnly(details.checked === true);
              reloadFromFirstPage();
            }}
            class="ark-checkbox h-10 rounded-xl border border-outline-variant bg-surface-container-high/50 px-3 text-label-large text-on-surface transition-colors hover:border-secondary/40"
          >
            <Checkbox.Control class="ark-checkbox__control">
              <Checkbox.Indicator class="ark-checkbox__indicator">
                <Check class="h-3.5 w-3.5" stroke-width={4} />
              </Checkbox.Indicator>
            </Checkbox.Control>
            <Checkbox.Label class="cursor-pointer select-none">
              Favorites Only
            </Checkbox.Label>
            <Checkbox.HiddenInput />
          </Checkbox.Root>
        </nav>
      </header>

      <div class="min-w-0">
        <Show
          when={readyState()}
          fallback={
            <LibraryStatusPanel
              title={statusTitle()}
              description={statusDescription()}
            />
          }
        >
          <section class="space-y-4" aria-labelledby="library-browse-title">
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <h2 id="library-browse-title" class="text-title-large">
                {libraryTitle(collectionType)}
              </h2>
              <p class="text-body-small">
                {readyState()?.items.length ?? 0} of{' '}
                {readyState()?.page.totalRecordCount ?? 0}
              </p>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 animate-fade-in">
              <For each={readyState()?.items ?? []}>
                {(item) => (
                  <VideoLibraryCard
                    item={item}
                    collectionType={collectionType}
                  />
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
                    <RefreshCw
                      class={`h-4 w-4 ${loading() ? 'animate-spin' : ''}`}
                    />
                  }
                >
                  {loading() ? 'Loading more' : 'Load more'}
                </Button>
              </div>
            </Show>
          </section>
        </Show>
      </div>
    </div>
  );
}
