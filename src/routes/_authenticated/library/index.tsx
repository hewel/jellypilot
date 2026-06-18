import {
  LibraryShortcutRow,
  LibraryStatusPanel,
  VideoHomeRow,
  VideoLibraryCard,
} from '@components/library/shared';
import { Button } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Library, RefreshCw, Search, X } from 'lucide-solid';
import { createResource, createSignal, For, Show } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import {
  fetchLibraryHome,
  fetchVideoSearchPage,
  type LibraryExit,
  type LibrarySearchState,
} from '~effects/library';

export const Route = createFileRoute('/_authenticated/library/')({
  component: LibraryLanding,
});

function LibraryLanding() {
  const [home, { refetch }] = createResource(fetchLibraryHome);
  const loadedHome = () => {
    const current = home();
    return current
      ? Exit.match(current, {
          onFailure: () => null,
          onSuccess: (v) => v,
        })
      : null;
  };

  // Lifted search state
  const [query, setQuery] = createSignal('');
  const [submittedQuery, setSubmittedQuery] = createSignal('');
  const [state, setState] =
    createSignal<LibraryExit<LibrarySearchState> | null>(null);
  const [loading, setLoading] = createSignal(false);

  const loadSearchPage = async (
    nextQuery: string,
    startIndex: number,
    replace = false,
  ) => {
    if (loading()) return;
    setLoading(true);
    const result = await fetchVideoSearchPage(nextQuery, startIndex);
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
  const renderHomeContent = () => {
    return (
      <div class="space-y-6">
        <VideoHomeRow
          id="continue-watching"
          title="Continue Watching"
          items={loadedHome()?.continueWatching ?? []}
        />
        <VideoHomeRow
          id="next-up"
          title="Next Up"
          items={loadedHome()?.nextUp ?? []}
        />
        <VideoHomeRow
          id="latest-movies"
          title="Latest Movies"
          items={loadedHome()?.latestMovies ?? []}
        />
        <VideoHomeRow
          id="latest-episodes"
          title="Latest Episodes"
          items={loadedHome()?.latestEpisodes ?? []}
        />
      </div>
    );
  };

  const renderSearchContent = () => {
    const error = searchErrorMessage();
    if (error) {
      return (
        <div class="space-y-3">
          <LibraryStatusPanel
            title="Could not search Library"
            description={error}
          />
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
          <h2 id="library-search-results" class="text-title-large">
            Search results
          </h2>
          <div class="flex items-center gap-3">
            <p class="text-body-small">
              {readyState()?.items.length ?? 0} of{' '}
              {readyState()?.page.totalRecordCount ?? 0} for "{submittedQuery()}
              "
            </p>
            <Button
              type="button"
              variant="text"
              size="sm"
              class="min-h-0 py-1 px-3 text-[12px] font-bold"
              onClick={clearSearch}
            >
              Clear Search
            </Button>
          </div>
        </div>
        <div class="grid gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 animate-fade-in">
          <For each={readyState()?.items ?? []}>
            {(item) => <VideoLibraryCard item={item} />}
          </For>
        </div>
        <Show when={readyState()?.page.hasMore}>
          <div class="flex justify-center pt-2">
            <Button
              type="button"
              variant="secondary"
              class="rounded-full"
              disabled={loading()}
              onClick={() =>
                void loadSearchPage(submittedQuery(), loadMoreStartIndex())
              }
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
      <header class="flex flex-col gap-4 rounded-2xl border border-outline-variant bg-surface-container-low/60 p-3 shadow-xl backdrop-blur-md sm:flex-row sm:items-center sm:justify-between lg:p-4">
        {/* Brand/Title */}
        <div class="flex items-center gap-3">
          <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary-container/30 text-primary">
            <Library class="h-5 w-5" />
          </div>
          <div>
            <h1 class="text-title-medium font-bold text-on-surface">Library</h1>
            <p class="text-[10px] font-semibold text-on-surface-variant/80">
              Jellyfin Media
            </p>
          </div>
        </div>

        {/* Search & Actions */}
        <div class="flex flex-col gap-3 sm:flex-row sm:items-center flex-1 max-w-2xl justify-end">
          <form
            onSubmit={(event) => {
              event.preventDefault();
              submitSearch();
            }}
            class="flex items-center gap-2 flex-1 max-w-md w-full"
            aria-label="Library search"
          >
            <div class="relative w-full">
              <label class="relative w-full block">
                <span class="sr-only">Search video library</span>
                <Search class="absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-on-surface-variant/60" />
                <input
                  type="text"
                  class="h-10 w-full rounded-xl border border-outline-variant/80 bg-surface-container-high/50 pl-10 pr-10 text-sm text-on-surface placeholder-on-surface-variant/50 outline-none backdrop-blur-sm transition-all duration-200 focus:border-secondary focus:bg-surface-container-high/80 focus:ring-2 focus:ring-secondary/15 disabled:cursor-not-allowed disabled:opacity-50"
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
                  class="absolute right-2.5 top-1/2 -translate-y-1/2 text-on-surface-variant/60 hover:text-on-surface p-1 rounded-lg transition-colors"
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
              class="h-10 rounded-xl px-4 shrink-0 font-bold"
              disabled={loading() || !query().trim()}
            >
              {loading() ? 'Searching' : 'Search'}
            </Button>
          </form>

          <Button
            type="button"
            variant="outlined"
            size="sm"
            class="h-10 rounded-xl px-4 shrink-0"
            onClick={() => void refetch()}
            disabled={home.loading}
            leadingIcon={
              <RefreshCw
                class={`h-4 w-4 ${home.loading ? 'animate-spin' : ''}`}
              />
            }
          >
            Retry Library
          </Button>
        </div>
      </header>

      <div class="space-y-6">
        <Show when={!isSearchActive() && !home.loading}>
          <LibraryShortcutRow
            shortcuts={loadedHome()?.libraryShortcuts ?? []}
            layout="grid"
          />
        </Show>

        <div class="min-w-0">
          {isSearchActive() ? renderSearchContent() : renderHomeContent()}
        </div>
      </div>
    </div>
  );
}
