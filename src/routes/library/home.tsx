import { RefreshCw } from 'lucide-solid';
import { createResource, createSignal, For, Show } from 'solid-js';
import {
  fetchLibraryHome,
  fetchVideoSearchPage,
  type LibrarySearchState,
} from './data';
import {
  LibraryShortcutRow,
  LibraryStatusPanel,
  VideoHomeRow,
  VideoLibraryCard,
} from './shared';

export function LibraryLanding() {
  const [home, { refetch }] = createResource(fetchLibraryHome);
  const loadedHome = () => {
    const current = home();
    return current?.kind === 'ready' ? current.home : null;
  };
  const statusTitle = () => {
    const current = home();
    if (current?.kind === 'empty') return 'Video Home has no video rows yet';
    if (current?.kind === 'error') return 'Could not load Library state';
    return 'Library requires a live Jellyfin connection';
  };
  const statusDescription = () => {
    const current = home();
    if (current?.kind === 'empty') {
      return 'Jellyfin returned no Continue Watching, Next Up, latest video rows, or video library shortcuts for this user.';
    }
    if (current?.kind === 'error') return current.message;
    return 'Reconnect Jellyfin to browse video libraries. Saved Sessions remain available, but Library data is not cached offline.';
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p class="text-label-small text-secondary">Library Browser</p>
          <h1 class="text-headline-large">Library</h1>
          <p class="mt-2 max-w-2xl text-body-large">
            Video Home will use live Jellyfin data for Continue Watching, Next
            Up, latest video rows, and library shortcuts.
          </p>
        </div>
        <button
          type="button"
          class="btn-outlined rounded-full"
          onClick={() => void refetch()}
          disabled={home.loading}
        >
          <RefreshCw class="h-4 w-4" />
          <span>Retry Library</span>
        </button>
      </div>

      <LibrarySearchPanel />

      <Show
        when={!home.loading}
        fallback={<LibraryStatusPanel title="Loading Video Home" />}
      >
        <Show
          when={home()?.kind === 'ready'}
          fallback={
            <LibraryStatusPanel
              title={statusTitle()}
              description={statusDescription()}
            />
          }
        >
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
            <LibraryShortcutRow
              shortcuts={loadedHome()?.libraryShortcuts ?? []}
            />
          </div>
        </Show>
      </Show>
    </div>
  );
}

function LibrarySearchPanel() {
  const [query, setQuery] = createSignal('');
  const [submittedQuery, setSubmittedQuery] = createSignal('');
  const [state, setState] = createSignal<LibrarySearchState | null>(null);
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
      if (!replace && current?.kind === 'ready' && result.kind === 'ready') {
        return {
          kind: 'ready',
          page: result.page,
          items: [...current.items, ...result.items],
        };
      }
      return result;
    });
    setLoading(false);
  };
  const submitSearch = () => {
    const nextQuery = query().trim();
    setSubmittedQuery(nextQuery);
    setState(null);
    void loadSearchPage(nextQuery, 0, true);
  };
  const readyState = () => {
    const current = state();
    return current?.kind === 'ready' ? current : null;
  };
  const statusTitle = () => {
    const current = state();
    if (!current) return loading() ? 'Searching Library' : null;
    if (current.kind === 'empty') return 'No video search results';
    if (current.kind === 'error') return 'Could not search Library';
    if (current.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return null;
  };
  const statusDescription = () => {
    const current = state();
    if (!current) return 'JMSR is searching Movies, Shows, and Episodes.';
    if (current.kind === 'empty') {
      return `Jellyfin returned no video results for "${current.page.query}".`;
    }
    if (current.kind === 'error') return current.message;
    if (current.kind === 'disconnected') {
      return 'Reconnect Jellyfin to search video libraries. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is searching Movies, Shows, and Episodes.';
  };
  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  return (
    <div class="space-y-4">
      <form
        class="card-filled flex flex-col gap-3 sm:flex-row sm:items-end"
        aria-label="Library search"
        onSubmit={(event) => {
          event.preventDefault();
          submitSearch();
        }}
      >
        <label class="min-w-0 flex-1 space-y-2">
          <span class="text-label-small">Search video library</span>
          <input
            class="input-filled w-full"
            value={query()}
            disabled={loading()}
            onInput={(event) => setQuery(event.currentTarget.value)}
          />
        </label>
        <button
          type="submit"
          class="btn-primary rounded-full"
          disabled={loading()}
        >
          <span>{loading() ? 'Searching' : 'Search'}</span>
        </button>
      </form>

      <Show when={state() !== null || loading()}>
        <Show
          when={state()?.kind === 'ready'}
          fallback={
            <div class="space-y-3">
              <LibraryStatusPanel
                title={statusTitle() ?? 'Searching Library'}
                description={statusDescription()}
              />
              <Show
                when={
                  submittedQuery() &&
                  (state()?.kind === 'error' ||
                    state()?.kind === 'disconnected')
                }
              >
                <button
                  type="button"
                  class="btn-secondary rounded-full"
                  disabled={loading()}
                  onClick={() => void loadSearchPage(submittedQuery(), 0, true)}
                >
                  <RefreshCw class="h-4 w-4" />
                  <span>Retry Search</span>
                </button>
              </Show>
            </div>
          }
        >
          <section class="space-y-4" aria-labelledby="library-search-results">
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <h2 id="library-search-results" class="text-title-large">
                Search results
              </h2>
              <p class="text-body-small">
                {readyState()?.items.length ?? 0} of{' '}
                {readyState()?.page.totalRecordCount ?? 0} for "
                {submittedQuery()}"
              </p>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              <For each={readyState()?.items ?? []}>
                {(item) => <VideoLibraryCard item={item} />}
              </For>
            </div>
            <Show when={readyState()?.page.hasMore}>
              <div class="flex justify-center pt-2">
                <button
                  type="button"
                  class="btn-secondary rounded-full"
                  disabled={loading()}
                  onClick={() =>
                    void loadSearchPage(submittedQuery(), loadMoreStartIndex())
                  }
                >
                  <RefreshCw class="h-4 w-4" />
                  <span>
                    {loading() ? 'Loading more' : 'Load more results'}
                  </span>
                </button>
              </div>
            </Show>
          </section>
        </Show>
      </Show>
    </div>
  );
}
