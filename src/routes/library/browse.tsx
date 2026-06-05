import { Select } from '@ark-ui/solid/select';
import { useParams } from '@tanstack/solid-router';
import { ChevronDown, Library, RefreshCw } from 'lucide-solid';
import { createSignal, For, onMount, Show } from 'solid-js';
import type {
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '../../bindings';
import { fetchVideoLibraryPage, type LibraryBrowseState } from './data';
import {
  LibraryStatusPanel,
  libraryTitle,
  playedFilterLabel,
  sortCollection,
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

      <section class="card-filled space-y-4" aria-label="Library controls">
        <div class="grid gap-4 lg:grid-cols-[minmax(180px,240px)_1fr_auto] lg:items-end">
          <Select.Root
            collection={sortCollection}
            closeOnSelect
            disabled={loading()}
            value={[sort()]}
            onValueChange={(details) => {
              if (details.value.length > 0) {
                setSort(details.value[0] as VideoLibrarySort);
                reloadFromFirstPage();
              }
            }}
            class="w-full"
          >
            <Select.Label class="mb-1.5 block text-label-small">
              Sort
            </Select.Label>
            <Select.Control class="select-filled flex w-full items-center">
              <Select.Trigger class="flex h-14 w-full items-center justify-between gap-2 rounded-2xl border border-outline-variant/80 bg-surface-container-highest/30 px-4 text-on-surface outline-none transition-all duration-200 hover:border-secondary/50 focus:border-secondary focus:ring-4 focus:ring-secondary/15 disabled:cursor-not-allowed disabled:opacity-50">
                <Select.ValueText
                  placeholder="Select sort…"
                  class="font-medium text-body-medium text-on-surface"
                />
                <Select.Indicator>
                  <ChevronDown class="h-4 w-4 text-on-surface-variant/70" />
                </Select.Indicator>
              </Select.Trigger>
            </Select.Control>
            <Select.Positioner>
              <Select.Content class="mt-2 rounded-2xl border border-outline-variant bg-surface-container-lowest p-2 shadow-2xl backdrop-blur-md max-h-60 overflow-y-auto z-50">
                <For each={sortCollection.items}>
                  {(item) => (
                    <Select.Item
                      item={item}
                      class="flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-body-medium text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface transition-colors"
                    >
                      <Select.ItemText class="font-medium">
                        {item.label}
                      </Select.ItemText>
                    </Select.Item>
                  )}
                </For>
              </Select.Content>
            </Select.Positioner>
            <Select.HiddenSelect />
          </Select.Root>
          <fieldset class="space-y-2" aria-label="Played filter">
            <legend class="text-label-small">Played filter</legend>
            <div class="flex flex-wrap gap-2">
              <For
                each={
                  ['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]
                }
              >
                {(filter) => (
                  <button
                    type="button"
                    class={`btn-outlined rounded-full ${
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
          <label class="inline-flex min-h-11 items-center gap-3 rounded-xl border border-outline-variant px-3 text-label-large">
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
            <span>Favorites</span>
          </label>
        </div>
      </section>

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
          <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
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
  );
}
