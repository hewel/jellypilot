import { useParams } from '@tanstack/solid-router';
import { Film, Library, RefreshCw, Tv } from 'lucide-solid';
import {
  createEffect,
  createResource,
  createSignal,
  For,
  Show,
} from 'solid-js';
import type { VideoLibraryItem, VideoSeason } from '../../bindings';
import { StatusBadge } from '../../components/ui';
import {
  fetchSeasonEpisodes,
  fetchVideoShowDetail,
  type SeasonEpisodesState,
  startLibraryPlayback,
} from './data';
import {
  formatRuntime,
  LibraryStatusPanel,
  seasonLabel,
  showSubtitle,
  UserDataControls,
} from './shared';

export function LibraryShowDetailRoute() {
  const params = useParams({ from: '/authenticated/library/shows/$seriesId' });

  return <LibraryShowDetailView seriesId={params().seriesId} />;
}

export function LibraryShowDetailView(props: { seriesId: string }) {
  const [state, { refetch }] = createResource(() =>
    fetchVideoShowDetail(props.seriesId),
  );
  const [selectedSeason, setSelectedSeason] = createSignal<VideoSeason | null>(
    null,
  );
  const [episodes, setEpisodes] = createSignal<SeasonEpisodesState | null>(
    null,
  );
  const [episodesLoading, setEpisodesLoading] = createSignal(false);
  const [playBusy, setPlayBusy] = createSignal(false);
  const [episodePlayBusy, setEpisodePlayBusy] = createSignal<string | null>(
    null,
  );
  const [playError, setPlayError] = createSignal<string | null>(null);
  const [autoLoaded, setAutoLoaded] = createSignal(false);
  const detail = () => {
    const current = state();
    return current?.kind === 'ready' ? current.detail : null;
  };
  const seasonEpisodes = () => {
    const current = episodes();
    return current?.kind === 'ready' ? current.page.episodes : [];
  };
  const loadEpisodes = async (season: VideoSeason) => {
    if (episodesLoading()) return;
    setSelectedSeason(season);
    setEpisodes(null);
    setEpisodesLoading(true);
    const result = await fetchSeasonEpisodes({
      seriesId: props.seriesId,
      seasonId: season.id,
      seasonNumber: season.seasonNumber,
    });
    setEpisodes(result);
    setEpisodesLoading(false);
  };
  // Auto-load the season containing the next-up episode via reactive effect
  createEffect(() => {
    const show = detail();
    if (!show || autoLoaded()) return;
    setAutoLoaded(true);
    if (show.nextEpisode?.seasonNumber != null) {
      const match = show.seasons.find(
        (s) => s.seasonNumber === show.nextEpisode?.seasonNumber,
      );
      if (match) {
        void loadEpisodes(match);
        return;
      }
    }
    // Fall back to first season
    if (show.seasons.length > 0) {
      void loadEpisodes(show.seasons[0]);
    }
  });
  const playShow = async () => {
    const show = detail();
    if (!show || playBusy()) return;

    setPlayBusy(true);
    setPlayError(null);
    const message = await startLibraryPlayback({
      itemId: show.id,
      mode: 'show',
      startPositionSeconds: null,
    });
    setPlayError(message);
    setPlayBusy(false);
  };
  const playEpisode = async (episode: VideoLibraryItem) => {
    if (episodePlayBusy()) return;
    const isResume =
      episode.resumePositionSeconds != null &&
      episode.resumePositionSeconds > 0 &&
      !episode.played;

    setEpisodePlayBusy(episode.id);
    setPlayError(null);
    const message = await startLibraryPlayback({
      itemId: episode.id,
      mode: isResume ? 'resume' : 'start',
      startPositionSeconds: isResume ? episode.resumePositionSeconds : 0,
    });
    setPlayError(message);
    setEpisodePlayBusy(null);
  };
  const statusTitle = () => {
    const current = state();
    if (current?.kind === 'error') return 'Could not load show detail';
    if (current?.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return 'Loading show detail';
  };
  const statusDescription = () => {
    const current = state();
    if (current?.kind === 'error') return current.message;
    if (current?.kind === 'disconnected') {
      return 'Reconnect Jellyfin to inspect show details. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is loading Show detail, seasons, and Jellyfin next-up data.';
  };
  const episodesStatusTitle = () => {
    const current = episodes();
    if (episodesLoading()) return 'Loading season episodes';
    if (current?.kind === 'empty') return 'Season has no episodes';
    if (current?.kind === 'error') return 'Could not load season episodes';
    return 'Choose a season';
  };
  const episodesStatusDescription = () => {
    const current = episodes();
    if (episodesLoading()) {
      return 'JMSR is loading exact Episode cards for the selected Season.';
    }
    if (current?.kind === 'empty') {
      return 'Jellyfin returned no Episodes for the selected Season.';
    }
    if (current?.kind === 'error') return current.message;
    return 'Season buttons keep manual episode selection available alongside Jellyfin next-up resolution.';
  };
  const episodeLabel = (ep: VideoLibraryItem) => {
    if (ep.seasonNumber != null && ep.episodeNumber != null) {
      return `S${ep.seasonNumber.toString().padStart(2, '0')}E${ep.episodeNumber.toString().padStart(2, '0')}`;
    }
    return 'Episode';
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
          <span>Retry Show</span>
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
        {(show) => (
          <div class="space-y-6">
            {/* Series header: poster + metadata + secondary Play next */}
            <article class="grid gap-6 lg:grid-cols-[minmax(180px,240px)_1fr]">
              <div class="card-filled overflow-hidden p-0">
                <div class="aspect-[2/3] bg-surface-container-lowest/60">
                  <Show
                    when={show().artworkUrl}
                    fallback={
                      <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-on-surface-variant">
                        <Tv class="h-8 w-8" />
                        <p class="text-title-medium">{show().name}</p>
                        <p class="text-label-small">No artwork</p>
                      </div>
                    }
                  >
                    {(artworkUrl) => (
                      <img
                        src={artworkUrl()}
                        alt={`${show().name} artwork`}
                        class="h-full w-full object-cover"
                      />
                    )}
                  </Show>
                </div>
              </div>
              <div class="space-y-4">
                <div>
                  <p class="text-label-small text-secondary">Series</p>
                  <h1 class="text-headline-large">{show().name}</h1>
                  <p class="mt-2 text-body-large">{showSubtitle(show())}</p>
                </div>
                <div class="flex flex-wrap gap-2">
                  <StatusBadge variant={show().played ? 'success' : 'neutral'}>
                    {show().played ? 'Played' : 'Unplayed'}
                  </StatusBadge>
                  <StatusBadge
                    variant={show().favorite ? 'success' : 'neutral'}
                  >
                    {show().favorite ? 'Favorite' : 'Not favorite'}
                  </StatusBadge>
                </div>
                <UserDataControls
                  itemId={show().id}
                  played={show().played}
                  favorite={show().favorite}
                  subject="show"
                  onSuccess={() => void refetch()}
                />
                <Show when={show().overview}>
                  {(overview) => <p class="text-body-medium">{overview()}</p>}
                </Show>
                <Show when={show().genres.length > 0}>
                  <div class="flex flex-wrap gap-2">
                    <For each={show().genres}>
                      {(genre) => (
                        <span class="rounded-full border border-outline-variant px-3 py-1 text-label-small">
                          {genre}
                        </span>
                      )}
                    </For>
                  </div>
                </Show>
                {/* Secondary Play next episode shortcut */}
                <Show when={show().nextEpisode}>
                  {(nextEpisode) => (
                    <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
                      <button
                        type="button"
                        class="btn-secondary rounded-full"
                        disabled={playBusy()}
                        onClick={() => void playShow()}
                      >
                        {playBusy() ? 'Starting' : 'Play next episode'}
                      </button>
                      <a
                        href={`/library/items/${nextEpisode().id}`}
                        class="text-body-small text-secondary underline-offset-4 hover:underline"
                      >
                        Next: {nextEpisode().name}
                      </a>
                    </div>
                  )}
                </Show>
              </div>
            </article>

            {/* Episodes-first: dominant season episode list */}
            <section class="space-y-4" aria-labelledby="show-seasons-title">
              <div class="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
                <div>
                  <h2 id="show-seasons-title" class="text-title-large">
                    Episodes
                  </h2>
                </div>
                <p class="text-body-small">
                  {show().seasons.length} seasons available
                </p>
              </div>

              <Show
                when={show().seasons.length > 0}
                fallback={
                  <LibraryStatusPanel
                    title="No seasons available"
                    description="Jellyfin returned no seasons for this show."
                  />
                }
              >
                <ul
                  class="flex gap-2 overflow-x-auto rounded-2xl border border-outline-variant bg-surface-container-low/70 p-2"
                  aria-label="Show seasons"
                >
                  <For each={show().seasons}>
                    {(season) => (
                      <li class="shrink-0">
                        <button
                          type="button"
                          class={`btn-outlined rounded-full ${
                            selectedSeason()?.id === season.id
                              ? 'border-secondary bg-secondary-container/45 text-on-secondary-container'
                              : ''
                          }`}
                          aria-pressed={selectedSeason()?.id === season.id}
                          disabled={episodesLoading()}
                          onClick={() => void loadEpisodes(season)}
                        >
                          <span>{seasonLabel(season)}</span>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>

                <Show
                  when={episodes()?.kind === 'ready'}
                  fallback={
                    <LibraryStatusPanel
                      title={episodesStatusTitle()}
                      description={episodesStatusDescription()}
                    />
                  }
                >
                  <section
                    class="space-y-2"
                    aria-labelledby="season-episodes-title"
                  >
                    <h3 id="season-episodes-title" class="text-title-medium">
                      {selectedSeason()
                        ? `${selectedSeason()?.name} Episodes`
                        : 'Episodes'}
                    </h3>
                    <For each={seasonEpisodes()}>
                      {(episode) => (
                        <div class="card-filled grid gap-3 p-3 sm:grid-cols-[minmax(120px,180px)_1fr]">
                          {/* Episode thumbnail - landscape, episode-specific */}
                          <div class="aspect-video overflow-hidden rounded-lg bg-surface-container-lowest/60">
                            <Show
                              when={episode.artworkUrl}
                              fallback={
                                <div class="flex h-full items-center justify-center text-label-small text-on-surface-variant">
                                  <Film class="h-5 w-5" />
                                </div>
                              }
                            >
                              {(artworkUrl) => (
                                <img
                                  src={artworkUrl()}
                                  alt={`${episode.name} artwork`}
                                  class="h-full w-full object-cover"
                                  loading="lazy"
                                />
                              )}
                            </Show>
                          </div>
                          {/* Episode metadata row */}
                          <div class="flex flex-col justify-between gap-2">
                            <div>
                              <div class="flex items-center gap-2">
                                <span class="text-label-small text-secondary">
                                  {episodeLabel(episode)}
                                </span>
                                <Show when={episode.played}>
                                  <StatusBadge variant="success">
                                    Played
                                  </StatusBadge>
                                </Show>
                                <Show when={episode.favorite}>
                                  <StatusBadge variant="success">
                                    Favorite
                                  </StatusBadge>
                                </Show>
                              </div>
                              <a
                                href={`/library/items/${episode.id}`}
                                class="text-title-medium hover:underline"
                              >
                                {episode.name}
                              </a>
                            </div>
                            <div class="flex flex-wrap items-center gap-2 text-body-small text-secondary">
                              <Show
                                when={formatRuntime(episode.runtimeSeconds)}
                              >
                                {(runtime) => <span>{runtime()}</span>}
                              </Show>
                              <Show
                                when={
                                  episode.resumePositionSeconds != null &&
                                  episode.resumePositionSeconds > 0 &&
                                  !episode.played
                                }
                              >
                                <span>
                                  {Math.round(episode.playedPercentage ?? 0)}%
                                  watched
                                </span>
                              </Show>
                              <div class="flex gap-2">
                                <Show
                                  when={
                                    episode.resumePositionSeconds != null &&
                                    episode.resumePositionSeconds > 0 &&
                                    !episode.played
                                  }
                                  fallback={
                                    <button
                                      type="button"
                                      class="btn-primary rounded-full px-3 py-1 text-label-small"
                                      disabled={episodePlayBusy() !== null}
                                      onClick={() => void playEpisode(episode)}
                                    >
                                      {episodePlayBusy() === episode.id
                                        ? 'Starting'
                                        : 'Play'}
                                    </button>
                                  }
                                >
                                  <button
                                    type="button"
                                    class="btn-primary rounded-full px-3 py-1 text-label-small"
                                    disabled={episodePlayBusy() !== null}
                                    onClick={() => void playEpisode(episode)}
                                  >
                                    {episodePlayBusy() === episode.id
                                      ? 'Starting'
                                      : 'Resume'}
                                  </button>
                                </Show>
                              </div>
                            </div>
                          </div>
                        </div>
                      )}
                    </For>
                  </section>
                </Show>
              </Show>
            </section>

            <Show when={playError()}>
              {(message) => (
                <p class="text-body-small text-error">{message()}</p>
              )}
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
