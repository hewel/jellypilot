import { useParams } from '@tanstack/solid-router';
import { Library, RefreshCw } from 'lucide-solid';
import { createSignal, For, onMount, Show } from 'solid-js';
import type {
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '../../bindings';
import { JmsrSelect } from '../../components/ui';
import { fetchVideoLibraryPage, type LibraryBrowseState } from './data';
import {
  LibraryStatusPanel,
  libraryTitle,
  playedFilterLabel,
  sortItems,
  VideoLibraryCard,
} from './shared';

function libraryKindFromParam(value: string): VideoLibraryKind {
  return value === 'tvshows' ? 'tvshows' : 'movies';
}

export function LibraryBrowseRoute() {
  const params = useParams({
    from: '/authenticated/library/$collectionType/$libraryId',
  });

  return (
    <LibraryBrowseView
      collectionType={libraryKindFromParam(params().collectionType)}
      libraryId={params().libraryId}
    />
  );
}

export function LibraryBrowseView(props: {
  collectionType: VideoLibraryKind;
  libraryId: string;
}) {
  const [state, setState] = createSignal<LibraryBrowseState | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [sort, setSort] = createSignal<VideoLibrarySort>('title');
  const [playedFilter, setPlayedFilter] =
    createSignal<VideoLibraryPlayedFilter>('all');
  const [favoritesOnly, setFavoritesOnly] = createSignal(false);

  const loadPage = async (startIndex: number, replace = false) => {
    if (loading()) return;
    setLoading(true);
    const result = await fetchVideoLibraryPage(
      props.collectionType,
      props.libraryId,
      startIndex,
      sort(),
      playedFilter(),
      favoritesOnly(),
    );
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
  const reloadFromFirstPage = () => {
    setState(null);
    void loadPage(0, true);
  };

  onMount(() => {
    void loadPage(0, true);
  });

  const readyState = () => {
    const current = state();
    return current?.kind === 'ready' ? current : null;
  };
  const statusTitle = () => {
    const current = state();
    if (!current) return `Loading ${libraryTitle(props.collectionType)}`;
    if (current.kind === 'empty') {
      return `${libraryTitle(props.collectionType)} has no results`;
    }
    if (current.kind === 'error') return 'Could not load Library page';
    if (current.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return `Loading ${libraryTitle(props.collectionType)}`;
  };
  const statusDescription = () => {
    const current = state();
    if (current?.kind === 'empty') {
      return 'Jellyfin returned an empty server page for this video library.';
    }
    if (current?.kind === 'error') return current.message;
    if (current?.kind === 'disconnected') {
      return 'Reconnect Jellyfin to browse video libraries. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is loading a server-paged video library result set.';
  };
  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p class="text-label-small text-secondary">Library Browser</p>
          <h1 class="text-headline-large">
            {libraryTitle(props.collectionType)}
          </h1>
          <p class="mt-2 max-w-2xl text-body-large">
            Server-paged video results from Jellyfin.
          </p>
        </div>
        <a href="/library" class="btn-outlined rounded-full">
          <Library class="h-4 w-4" />
          <span>Video Home</span>
        </a>
      </div>

      <div class="console-grid">
        {/* Left Column: browse results */}
        <div class="min-w-0">
          <Show
            when={state()?.kind === 'ready'}
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
                  {libraryTitle(props.collectionType)}
                </h2>
                <p class="text-body-small">
                  {readyState()?.items.length ?? 0} of{' '}
                  {readyState()?.page.totalRecordCount ?? 0}
                </p>
              </div>
              <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3 animate-fade-in">
                <For each={readyState()?.items ?? []}>
                  {(item) => (
                    <VideoLibraryCard
                      item={item}
                      collectionType={props.collectionType}
                    />
                  )}
                </For>
              </div>
              <Show when={readyState()?.page.hasMore}>
                <div class="flex justify-center pt-2">
                  <button
                    type="button"
                    class="btn-secondary rounded-full"
                    disabled={loading()}
                    onClick={() => void loadPage(loadMoreStartIndex())}
                  >
                    <RefreshCw class="h-4 w-4" />
                    <span>{loading() ? 'Loading more' : 'Load more'}</span>
                  </button>
                </div>
              </Show>
            </section>
          </Show>
        </div>

        {/* Right Column: controls sidebar */}
        <aside class="space-y-6">
          <section class="card-filled space-y-5" aria-label="Library controls">
            <h2 class="text-title-medium">Filters & Sort</h2>

            <JmsrSelect
              label="Sort By"
              items={sortItems}
              disabled={loading()}
              value={sort()}
              placeholder="Select sort…"
              onValueChange={(value) => {
                setSort(value);
                reloadFromFirstPage();
              }}
              class="w-full"
            />

            <fieldset class="space-y-2" aria-label="Played filter">
              <legend class="text-label-small">Status</legend>
              <div class="flex flex-col gap-2">
                <For
                  each={
                    ['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]
                  }
                >
                  {(filter) => (
                    <button
                      type="button"
                      class={`btn-outlined rounded-full w-full justify-start ${
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
                    </button>
                  )}
                </For>
              </div>
            </fieldset>

            <label class="inline-flex min-h-11 w-full items-center gap-3 rounded-xl border border-outline-variant px-3 py-2 text-label-large cursor-pointer hover:border-secondary/40 transition-colors">
              <input
                type="checkbox"
                class="h-4 w-4 accent-secondary"
                checked={favoritesOnly()}
                disabled={loading()}
                onChange={(event) => {
                  setFavoritesOnly(event.currentTarget.checked);
                  reloadFromFirstPage();
                }}
              />
              <span>Favorites Only</span>
            </label>
          </section>
        </aside>
      </div>
    </div>
  );
}
