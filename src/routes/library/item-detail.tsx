import { useParams } from '@tanstack/solid-router';
import { Film, Library, RefreshCw } from 'lucide-solid';
import { createResource, createSignal, For, Show } from 'solid-js';
import type { VideoLibraryPlayMode } from '../../bindings';
import { StatusBadge } from '../../components/ui';
import { fetchVideoItemDetail, startLibraryPlayback } from './data';
import {
  detailSubtitle,
  formatRuntime,
  LibraryStatusPanel,
  UserDataControls,
} from './shared';

export function LibraryItemDetailRoute() {
  const params = useParams({ from: '/authenticated/library/items/$itemId' });

  return <LibraryItemDetailView itemId={params().itemId} />;
}

export function LibraryItemDetailView(props: { itemId: string }) {
  const [state, { refetch }] = createResource(() =>
    fetchVideoItemDetail(props.itemId),
  );
  const [playBusy, setPlayBusy] = createSignal<VideoLibraryPlayMode | null>(
    null,
  );
  const [playError, setPlayError] = createSignal<string | null>(null);
  const detail = () => {
    const current = state();
    return current?.kind === 'ready' ? current.detail : null;
  };
  const playItem = async (mode: VideoLibraryPlayMode) => {
    const item = detail();
    if (!item || playBusy()) return;

    setPlayBusy(mode);
    setPlayError(null);
    const message = await startLibraryPlayback({
      itemId: item.id,
      mode,
      startPositionSeconds: mode === 'resume' ? item.resumePositionSeconds : 0,
    });
    setPlayError(message);
    setPlayBusy(null);
  };
  const statusTitle = () => {
    const current = state();
    if (current?.kind === 'error') return 'Could not load item detail';
    if (current?.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return 'Loading item detail';
  };
  const statusDescription = () => {
    const current = state();
    if (current?.kind === 'error') return current.message;
    if (current?.kind === 'disconnected') {
      return 'Reconnect Jellyfin to inspect video details. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is loading Movie or Episode detail data from Jellyfin.';
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <a href="/library" class="btn-outlined rounded-full">
          <Library class="h-4 w-4" />
          <span>Video Home</span>
        </a>
        <button
          type="button"
          class="btn-outlined rounded-full"
          disabled={state.loading}
          onClick={() => void refetch()}
        >
          <RefreshCw class="h-4 w-4" />
          <span>Retry Detail</span>
        </button>
      </div>

      <Show
        when={detail()}
        fallback={
          <LibraryStatusPanel
            title={statusTitle()}
            description={statusDescription()}
          />
        }
      >
        {(item) => {
          const isEpisode = () => item().itemType === 'Episode';

          return (
            <article class="grid gap-6 lg:grid-cols-[minmax(240px,360px)_1fr]">
              <div class="card-filled overflow-hidden p-0">
                <Show
                  when={isEpisode()}
                  fallback={
                    <div class="aspect-[2/3] bg-surface-container-lowest/60">
                      <Show
                        when={item().artworkUrl}
                        fallback={
                          <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-on-surface-variant">
                            <Film class="h-8 w-8" />
                            <p class="text-title-medium">{item().name}</p>
                            <p class="text-label-small">No artwork</p>
                          </div>
                        }
                      >
                        {(artworkUrl) => (
                          <img
                            src={artworkUrl()}
                            alt={`${item().name} artwork`}
                            class="h-full w-full object-cover"
                          />
                        )}
                      </Show>
                    </div>
                  }
                >
                  {/* Episode: landscape thumbnail header */}
                  <div class="aspect-video bg-surface-container-lowest/60">
                    <Show
                      when={item().artworkUrl}
                      fallback={
                        <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-on-surface-variant">
                          <Film class="h-8 w-8" />
                          <p class="text-title-medium">{item().name}</p>
                          <p class="text-label-small">No episode artwork</p>
                        </div>
                      }
                    >
                      {(artworkUrl) => (
                        <img
                          src={artworkUrl()}
                          alt={`${item().name} artwork`}
                          class="h-full w-full object-cover"
                        />
                      )}
                    </Show>
                  </div>
                </Show>
              </div>
              <div class="space-y-5">
                <div>
                  <p class="text-label-small text-secondary">
                    {item().itemType}
                  </p>
                  <h1 class="text-headline-large">{item().name}</h1>
                  <p class="mt-2 text-body-large">{detailSubtitle(item())}</p>
                  <Show when={isEpisode() && item().seriesId}>
                    <a
                      href={`/library/shows/${item().seriesId}`}
                      class="mt-1 inline-block text-body-small text-secondary underline-offset-4 hover:underline"
                    >
                      View series
                    </a>
                  </Show>
                </div>
                <div class="flex flex-wrap gap-2">
                  <StatusBadge variant={item().played ? 'success' : 'neutral'}>
                    {item().played ? 'Played' : 'Unplayed'}
                  </StatusBadge>
                  <StatusBadge
                    variant={item().favorite ? 'success' : 'neutral'}
                  >
                    {item().favorite ? 'Favorite' : 'Not favorite'}
                  </StatusBadge>
                  <Show when={formatRuntime(item().runtimeSeconds)}>
                    {(runtime) => (
                      <StatusBadge variant="neutral">{runtime()}</StatusBadge>
                    )}
                  </Show>
                </div>
                <UserDataControls
                  itemId={item().id}
                  played={item().played}
                  favorite={item().favorite}
                  subject={item().itemType.toLowerCase()}
                  onSuccess={() => void refetch()}
                />
                <Show when={item().overview}>
                  {(overview) => <p class="text-body-medium">{overview()}</p>}
                </Show>
                <Show when={item().genres.length > 0}>
                  <div class="flex flex-wrap gap-2">
                    <For each={item().genres}>
                      {(genre) => (
                        <span class="rounded-full border border-outline-variant px-3 py-1 text-label-small">
                          {genre}
                        </span>
                      )}
                    </For>
                  </div>
                </Show>
                <Show when={item().resumePositionSeconds !== null}>
                  <p class="text-body-small text-secondary">
                    Resume at {Math.floor(item().resumePositionSeconds ?? 0)}s
                    {item().playedPercentage !== null
                      ? ` · ${Math.round(item().playedPercentage ?? 0)}% watched`
                      : ''}
                  </p>
                </Show>
                <div class="flex flex-wrap gap-3">
                  <Show
                    when={item().canResume}
                    fallback={
                      <button
                        type="button"
                        class="btn-primary rounded-full"
                        disabled={playBusy() !== null}
                        onClick={() => void playItem('start')}
                      >
                        {playBusy() === 'start' ? 'Starting' : 'Play'}
                      </button>
                    }
                  >
                    <button
                      type="button"
                      class="btn-primary rounded-full"
                      disabled={playBusy() !== null}
                      onClick={() => void playItem('resume')}
                    >
                      {playBusy() === 'resume' ? 'Starting' : 'Resume'}
                    </button>
                    <button
                      type="button"
                      class="btn-secondary rounded-full"
                      disabled={playBusy() !== null}
                      onClick={() => void playItem('start')}
                    >
                      {playBusy() === 'start'
                        ? 'Starting'
                        : 'Play from beginning'}
                    </button>
                  </Show>
                </div>
                <Show when={playError()}>
                  {(message) => (
                    <p class="text-body-small text-error">{message()}</p>
                  )}
                </Show>
              </div>
            </article>
          );
        }}
      </Show>
    </div>
  );
}
