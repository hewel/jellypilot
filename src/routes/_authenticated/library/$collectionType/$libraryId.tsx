import { Checkbox } from '@ark-ui/solid/checkbox';
import type { VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibrarySort } from '@bindings';
import {
  LibraryStatusPanel,
  MediaInfoHoverCard,
  VideoLibraryCard,
  libraryTitle,
  playedFilterLabel,
  sortItems,
} from '@components/library/shared';
import { Button, Card, JmsrSelect } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Check, Library, RefreshCw } from 'lucide-solid';
import { For, Show, Suspense, createResource, createSignal } from 'solid-js';
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
    return current && Exit.isSuccess(current) ? current.value : null;
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
    <div class="space-y-6">
      <header class="border-outline-variant bg-surface-container-low/60 flex flex-col gap-4 rounded-2xl border p-3 shadow-xl backdrop-blur-md lg:p-4">
        <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div class="flex items-center gap-3">
            <div class="border-primary/20 bg-primary-container/30 text-primary flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border">
              <Library class="h-5 w-5" />
            </div>
            <div>
              <h1 class="text-on-surface text-[16px] leading-[24px] font-bold font-semibold">
                {libraryTitle(collectionType())}
              </h1>
              <p class="text-on-surface-variant/80 text-[10px] font-semibold">Library Browser</p>
            </div>
          </div>

          <Button
            href="/library"
            variant="outlined"
            size="sm"
            class="h-10 shrink-0 rounded-xl px-4"
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
              <legend class="text-on-surface-variant text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                Status
              </legend>
              <div class="flex flex-wrap gap-2">
                <For each={['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]}>
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
            class="border-outline-variant bg-surface-container-high/50 text-on-surface hover:border-secondary/40 inline-flex h-10 cursor-pointer items-center gap-2.5 rounded-xl border px-3 align-top text-[14px] leading-[20px] font-semibold tracking-wide uppercase transition-colors transition-opacity select-none disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Checkbox.Control class="border-outline bg-surface-container-high text-on-primary hover:border-primary/60 data-[state=checked]:border-primary data-[state=checked]:from-primary data-[state=checked]:to-primary-gradient-end data-[state=indeterminate]:border-primary data-[state=indeterminate]:from-primary data-[state=indeterminate]:to-primary-gradient-end data-[focus-visible]:ring-primary/50 data-[focus-visible]:ring-offset-background inline-flex h-5.5 w-5.5 shrink-0 items-center justify-center rounded-lg border text-[11px] leading-none transition-all duration-200 data-[focus-visible]:ring-2 data-[focus-visible]:ring-offset-2 data-[focus-visible]:outline-none data-[state=checked]:bg-gradient-to-br data-[state=indeterminate]:bg-gradient-to-br">
              <Checkbox.Indicator class="flex items-center justify-center font-black">
                <Check class="h-3.5 w-3.5" stroke-width={4} />
              </Checkbox.Indicator>
            </Checkbox.Control>
            <Checkbox.Label class="cursor-pointer select-none">Favorites Only</Checkbox.Label>
            <Checkbox.HiddenInput />
          </Checkbox.Root>
        </nav>
      </header>

      <div class="min-w-0">
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
                    leadingIcon={<RefreshCw class={`h-4 w-4 ${loading() ? 'animate-spin' : ''}`} />}
                  >
                    {loading() ? 'Loading more' : 'Load more'}
                  </Button>
                </div>
              </Show>
            </section>
          </Show>
        </Suspense>
      </div>
    </div>
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
