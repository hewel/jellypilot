import {
  LibraryShortcutRow,
  LibraryStatusPanel,
  VideoHomeRow,
  VideoLibraryCard,
} from '@components/library/shared';
import { Button, Card } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Library, RefreshCw, Search, X } from 'lucide-solid';
import { For, Show, Suspense, createEffect, createResource, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { defaultTo } from '~effects/helper';
import { fetchLibraryHome, fetchLibraryShortcuts, fetchVideoSearchPage } from '~effects/library';
import type { LibraryExit, LibraryHomeState, LibrarySearchState } from '~effects/library';

const homeSkeletonRows = [
  { id: 'continue-watching-skeleton', aspectClass: 'aspect-video' },
  { id: 'next-up-skeleton', aspectClass: 'aspect-video' },
  { id: 'latest-movies-skeleton', aspectClass: 'aspect-[2/3]' },
  { id: 'latest-episodes-skeleton', aspectClass: 'aspect-video' },
] as const;

export const Route = createFileRoute('/_authenticated/library/')({
  component: LibraryLanding,
  loader: async () => {
    const shortcuts = await fetchLibraryShortcuts().then(defaultTo([]));
    const home = fetchLibraryHome().then(defaultTo(null));

    return { home, shortcuts };
  },
});

function LibraryLanding() {
  const loaderData = Route.useLoaderData();
  const [homePromise, setHomePromise] = createSignal<Promise<LibraryHomeState | null>>(
    loaderData().home,
  );
  const [home] = createResource(homePromise, (promise) => promise);
  const libraryShortcuts = () => loaderData().shortcuts;

  createEffect(() => {
    setHomePromise(loaderData().home);
  });

  const retryLibrary = () => {
    setHomePromise(fetchLibraryHome().then(defaultTo(null)));
  };

  // Lifted search state
  const [query, setQuery] = createSignal('');
  const [submittedQuery, setSubmittedQuery] = createSignal('');
  const [state, setState] = createSignal<LibraryExit<LibrarySearchState> | null>(null);
  const [loading, setLoading] = createSignal(false);

  const loadSearchPage = async (nextQuery: string, startIndex: number, replace = false) => {
    if (loading()) {
      return;
    }
    setLoading(true);
    const result = await fetchVideoSearchPage(nextQuery, startIndex);
    setState((current) => {
      if (!replace && current && Exit.isSuccess(current) && Exit.isSuccess(result)) {
        return Exit.succeed({
          items: [...current.value.items, ...result.value.items],
          page: result.value.page,
        });
      }
      return result;
    });
    setLoading(false);
  };

  const submitSearch = () => {
    const nextQuery = query().trim();
    if (!nextQuery) {
      clearSearch();
      return;
    }
    setSubmittedQuery(nextQuery);
    setState(null);
    void loadSearchPage(nextQuery, 0, true);
  };

  const clearSearch = () => {
    setQuery('');
    setSubmittedQuery('');
    setState(null);
  };

  const readyState = () => {
    const current = state();
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const searchErrorMessage = () => {
    const current = state();
    return current && !Exit.isSuccess(current)
      ? commandFailureMessage(current.cause, 'Could not search Library')
      : null;
  };

  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  const isSearchActive = () => state() !== null || loading();
  const renderHomeContent = () => (
    <Suspense fallback={<VideoHomeSkeleton />}>
      <div class="space-y-6">
        <VideoHomeRow
          id="continue-watching"
          title="Continue Watching"
          kind="continueWatching"
          items={home()?.continueWatching ?? []}
        />
        <VideoHomeRow id="next-up" title="Next Up" kind="nextUp" items={home()?.nextUp ?? []} />
        <VideoHomeRow
          id="latest-movies"
          title="Latest Movies"
          kind="latestMovies"
          items={home()?.latestMovies ?? []}
        />
        <VideoHomeRow
          id="latest-episodes"
          title="Latest Episodes"
          kind="latestEpisodes"
          items={home()?.latestEpisodes ?? []}
        />
      </div>
    </Suspense>
  );

  const renderSearchContent = () => {
    const error = searchErrorMessage();
    if (error) {
      return (
        <div class="space-y-3">
          <LibraryStatusPanel title="Could not search Library" description={error} />
          <Show when={submittedQuery()}>
            <Button
              type="button"
              variant="secondary"
              class="rounded-full"
              disabled={loading()}
              onClick={() => void loadSearchPage(submittedQuery(), 0, true)}
              leadingIcon={<RefreshCw class="h-4 w-4" />}
            >
              Retry Search
            </Button>
          </Show>
        </div>
      );
    }
    const ready = readyState();
    if (ready && ready.items.length === 0) {
      return <LibraryStatusPanel title="No video search results" />;
    }
    if (loading() && !readyState()) {
      return <LibraryStatusPanel title="Searching Library" />;
    }
    return (
      <section class="space-y-4" aria-labelledby="library-search-results">
        <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <h2
            id="library-search-results"
            class="text-on-surface text-[22px] leading-[28px] font-bold"
          >
            Search results
          </h2>
          <div class="flex items-center gap-3">
            <p class="text-on-surface-variant/80 text-[12px] leading-[16px]">
              {readyState()?.items.length ?? 0} of {readyState()?.page.totalRecordCount ?? 0} for "
              {submittedQuery()}"
            </p>
            <Button
              type="button"
              variant="text"
              size="sm"
              class="min-h-0 px-3 py-1 text-[12px] font-bold"
              onClick={clearSearch}
            >
              Clear Search
            </Button>
          </div>
        </div>
        <div class="grid animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          <For each={readyState()?.items ?? []}>{(item) => <VideoLibraryCard item={item} />}</For>
        </div>
        <Show when={readyState()?.page.hasMore}>
          <div class="flex justify-center pt-2">
            <Button
              type="button"
              variant="secondary"
              class="rounded-full"
              disabled={loading()}
              onClick={() => void loadSearchPage(submittedQuery(), loadMoreStartIndex())}
              leadingIcon={<RefreshCw class="h-4 w-4" />}
            >
              {loading() ? 'Loading more' : 'Load more results'}
            </Button>
          </div>
        </Show>
      </section>
    );
  };

  return (
    <div class="space-y-6">
      {/* Top sub-navbar */}
      <header class="border-outline-variant bg-surface-container-low/60 flex flex-col gap-4 rounded-2xl border p-3 shadow-xl backdrop-blur-md sm:flex-row sm:items-center sm:justify-between lg:p-4">
        {/* Brand/Title */}
        <div class="flex items-center gap-3">
          <div class="border-primary/20 bg-primary-container/30 text-primary flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border">
            <Library class="h-5 w-5" />
          </div>
          <div>
            <h1 class="text-on-surface text-[16px] leading-[24px] font-bold font-semibold">
              Library
            </h1>
            <p class="text-on-surface-variant/80 text-[10px] font-semibold">Jellyfin Media</p>
          </div>
        </div>

        {/* Search & Actions */}
        <div class="flex max-w-2xl flex-1 flex-col justify-end gap-3 sm:flex-row sm:items-center">
          <form
            onSubmit={(event) => {
              event.preventDefault();
              submitSearch();
            }}
            class="flex w-full max-w-md flex-1 items-center gap-2"
            aria-label="Library search"
          >
            <div class="relative w-full">
              <label class="relative block w-full">
                <span class="sr-only">Search video library</span>
                <Search class="text-on-surface-variant/60 absolute top-1/2 left-3.5 h-4 w-4 -translate-y-1/2" />
                <input
                  type="text"
                  class="border-outline-variant/80 bg-surface-container-high/50 text-on-surface placeholder-on-surface-variant/50 focus:border-secondary focus:bg-surface-container-high/80 focus:ring-secondary/15 h-10 w-full rounded-xl border pr-10 pl-10 text-sm backdrop-blur-sm transition-all duration-200 outline-none focus:ring-2 disabled:cursor-not-allowed disabled:opacity-50"
                  value={query()}
                  disabled={loading()}
                  onInput={(event) => setQuery(event.currentTarget.value)}
                  placeholder="Search movies, shows..."
                />
              </label>
              <Show when={query()}>
                <button
                  type="button"
                  onClick={clearSearch}
                  class="text-on-surface-variant/60 hover:text-on-surface absolute top-1/2 right-2.5 -translate-y-1/2 rounded-lg p-1 transition-colors"
                  title="Clear search"
                >
                  <X class="h-4 w-4" />
                </button>
              </Show>
            </div>
            <Button
              type="submit"
              variant="primary"
              size="sm"
              class="h-10 shrink-0 rounded-xl px-4 font-bold"
              disabled={loading() || !query().trim()}
            >
              {loading() ? 'Searching' : 'Search'}
            </Button>
          </form>

          <Button
            type="button"
            variant="outlined"
            size="sm"
            class="h-10 shrink-0 rounded-xl px-4"
            onClick={retryLibrary}
            disabled={home.loading}
            leadingIcon={<RefreshCw class={`h-4 w-4 ${home.loading ? 'animate-spin' : ''}`} />}
          >
            Retry Library
          </Button>
        </div>
      </header>

      <div class="space-y-6">
        <LibraryShortcutRow shortcuts={libraryShortcuts()} layout="grid" />

        <div class="min-w-0">{isSearchActive() ? renderSearchContent() : renderHomeContent()}</div>
      </div>
    </div>
  );
}

function VideoHomeSkeleton() {
  return (
    <div class="space-y-6" aria-hidden="true">
      <For each={homeSkeletonRows}>
        {(row) => (
          <section class="space-y-3">
            <div class="bg-surface-container-high/70 h-6 w-44 animate-pulse rounded-md" />
            <div class="grid gap-3 sm:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-6">
              <For each={[0, 1, 2, 3]}>
                {() => (
                  <Card variant="filled" surfaceTint={false} class="overflow-hidden !p-0">
                    <div
                      class={`${row.aspectClass} border-outline-variant bg-surface-container-lowest/60 animate-pulse border-b`}
                    />
                    <div class="space-y-2 px-4 pt-2 pb-3">
                      <div class="bg-surface-container-high/80 h-4 w-4/5 animate-pulse rounded" />
                      <div class="bg-surface-container-high/60 h-3 w-3/5 animate-pulse rounded" />
                    </div>
                  </Card>
                )}
              </For>
            </div>
          </section>
        )}
      </For>
    </div>
  );
}
