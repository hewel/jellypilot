import type { VideoLibraryItem, VideoSeason } from '@bindings';
import { LibraryPlaybackChooser } from '@components/library/LibraryPlaybackChooser';
import type {
  LibraryPlaybackSelection,
  PendingLibraryPlayback,
} from '@components/library/LibraryPlaybackChooser';
import {
  LibraryStatusPanel,
  UserDataControls,
  formatRuntime,
  seasonLabel,
  showSubtitle,
} from '@components/library/shared';
import { Button, Card, ConsoleGrid, StatusBadge } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Film, Library, RefreshCw, Tv } from 'lucide-solid';
import { For, Show, Suspense, createEffect, createResource, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import {
  fetchInitialSeasonEpisodes,
  fetchSeasonEpisodes,
  fetchVideoItemDetail,
  fetchVideoShowDetail,
  initialSeasonForShow,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import type { LibraryExit, LibraryShowState, SeasonEpisodesState } from '~effects/library';

export const Route = createFileRoute('/_authenticated/library/shows/$seriesId')({
  loader: ({ params }) => {
    const show = fetchVideoShowDetail(params.seriesId);

    return {
      initialEpisodes: fetchInitialSeasonEpisodes(params.seriesId, show),
      show,
    };
  },
  component: LibraryShowDetailRoute,
});

function LibraryShowDetailRoute() {
  const params = Route.useParams();
  const loaderData = Route.useLoaderData();
  const [manualShowPromise, setManualShowPromise] = createSignal<Promise<
    LibraryExit<LibraryShowState>
  > | null>(null);
  const [manualInitialEpisodesPromise, setManualInitialEpisodesPromise] =
    createSignal<Promise<LibraryExit<SeasonEpisodesState> | null> | null>(null);
  const [state] = createResource(
    () => manualShowPromise() ?? loaderData().show,
    (promise) => promise,
  );
  const [initialEpisodes] = createResource(
    () => manualInitialEpisodesPromise() ?? loaderData().initialEpisodes,
    (promise) => promise,
  );
  const [selectedSeason, setSelectedSeason] = createSignal<VideoSeason | null>(null);
  const [episodes, setEpisodes] = createSignal<LibraryExit<SeasonEpisodesState> | null>(null);
  const [episodesLoading, setEpisodesLoading] = createSignal(false);
  const [playBusy, setPlayBusy] = createSignal(false);
  const [episodePlayBusy, setEpisodePlayBusy] = createSignal<string | null>(null);
  const [confirmBusy, setConfirmBusy] = createSignal(false);
  const [pendingPlayback, setPendingPlayback] = createSignal<PendingLibraryPlayback | null>(null);
  const [playError, setPlayError] = createSignal<string | null>(null);

  createEffect(() => {
    params().seriesId;
    setManualShowPromise(null);
    setManualInitialEpisodesPromise(null);
    setSelectedSeason(null);
    setEpisodes(null);
  });

  const reloadShow = () => {
    const show = fetchVideoShowDetail(params().seriesId);
    setManualShowPromise(show);
    setManualInitialEpisodesPromise(fetchInitialSeasonEpisodes(params().seriesId, show));
    setSelectedSeason(null);
    setEpisodes(null);
  };

  const detail = () => {
    const current = state.latest ?? state();
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const activeSeason = () => {
    const selected = selectedSeason();
    if (selected) {
      return selected;
    }

    const show = detail();
    return show ? initialSeasonForShow(show) : null;
  };
  const currentEpisodes = () => (selectedSeason() ? episodes() : (initialEpisodes() ?? null));
  const seasonEpisodes = () => {
    const current = currentEpisodes();
    return current && Exit.isSuccess(current) ? current.value.page.episodes : [];
  };
  const hasSeasonEpisodes = () => seasonEpisodes().length > 0;
  const loadEpisodes = async (season: VideoSeason) => {
    if (episodesLoading()) {
      return;
    }
    setSelectedSeason(season);
    setEpisodes(null);
    setEpisodesLoading(true);
    const result = await fetchSeasonEpisodes({
      seasonId: season.id,
      seasonNumber: season.seasonNumber,
      seriesId: params().seriesId,
    });
    setEpisodes(result);
    setEpisodesLoading(false);
  };
  const openEpisodePlaybackChooser = async (itemId: string) => {
    const result = await fetchVideoItemDetail(itemId);
    Exit.match(result, {
      onFailure: (cause) => setPlayError(commandFailureMessage(cause, 'Could not load episode')),
      onSuccess: (episodeDetail) => {
        const mode = episodeDetail.canResume ? 'resume' : 'start';
        setPendingPlayback({
          detail: episodeDetail,
          mode,
          startPositionSeconds: mode === 'resume' ? episodeDetail.resumePositionSeconds : 0,
        });
      },
    });
  };
  const playShow = async () => {
    const show = detail();
    if (!show?.nextEpisode || playBusy() || confirmBusy()) {
      return;
    }

    setPlayBusy(true);
    setPlayError(null);
    await openEpisodePlaybackChooser(show.nextEpisode.id);
    setPlayBusy(false);
  };
  const playEpisode = async (episode: VideoLibraryItem) => {
    if (episodePlayBusy() || confirmBusy()) {
      return;
    }

    setEpisodePlayBusy(episode.id);
    setPlayError(null);
    await openEpisodePlaybackChooser(episode.id);
    setEpisodePlayBusy(null);
  };
  const confirmPlayback = async (selection: LibraryPlaybackSelection) => {
    const pending = pendingPlayback();
    if (!pending || confirmBusy()) {
      return;
    }

    setConfirmBusy(true);
    setPlayError(null);
    const result = await startLibraryPlayback({
      audioStreamIndex: selection.audioStreamIndex,
      itemId: pending.detail.id,
      mode: pending.mode,
      startPositionSeconds: pending.startPositionSeconds,
      subtitleStreamIndex: selection.subtitleStreamIndex,
    });
    const message = Exit.match(result, {
      onFailure: (cause) => commandFailureMessage(cause, 'Could not start playback'),
      onSuccess: () => null,
    });
    setPlayError(message);
    setConfirmBusy(false);
    if (!message) {
      setPendingPlayback(null);
    }
  };
  const statusTitle = () => {
    const current = state.latest ?? state();
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load show detail';
    }
    return 'Loading show detail';
  };
  const statusDescription = () => {
    const current = state.latest ?? state();
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load show detail');
    }
    return 'JMSR is loading Show detail, seasons, and Jellyfin next-up data.';
  };
  const episodesStatusTitle = () => {
    const current = currentEpisodes();
    if (episodesLoading()) {
      return 'Loading season episodes';
    }
    if (current && Exit.isSuccess(current) && current.value.page.episodes.length === 0) {
      return 'Season has no episodes';
    }
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load season episodes';
    }
    return 'Choose a season';
  };
  const episodesStatusDescription = () => {
    const current = currentEpisodes();
    if (episodesLoading()) {
      return 'JMSR is loading exact Episode cards for the selected Season.';
    }
    if (current && Exit.isSuccess(current) && current.value.page.episodes.length === 0) {
      return 'Jellyfin returned no Episodes for the selected Season.';
    }
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load season episodes');
    }
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
        <Button
          href="/library"
          variant="outlined"
          class="rounded-full"
          leadingIcon={<Library class="h-4 w-4" />}
        >
          Video Home
        </Button>
        <Button
          type="button"
          variant="outlined"
          class="rounded-full"
          disabled={state.loading}
          onClick={reloadShow}
          leadingIcon={<RefreshCw class="h-4 w-4" classList={{ 'animate-spin': state.loading }} />}
        >
          Retry Show
        </Button>
      </div>

      <Suspense fallback={<ShowDetailSkeleton />}>
        <Show
          when={detail()}
          fallback={<LibraryStatusPanel title={statusTitle()} description={statusDescription()} />}
        >
          {(show) => (
            <ConsoleGrid>
              {/* Left Column (Interactive): Seasons & Episode List */}
              <section
                class="order-2 min-w-0 space-y-4 lg:order-1"
                aria-labelledby="show-seasons-title"
              >
                <div class="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
                  <div>
                    <h2
                      id="show-seasons-title"
                      class="text-on-surface text-[22px] leading-[28px] font-bold"
                    >
                      Episodes
                    </h2>
                  </div>
                  <p class="text-on-surface-variant/80 text-[12px] leading-[16px]">
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
                    class="border-outline-variant bg-surface-container-low/70 flex gap-2 overflow-x-auto rounded-2xl border p-2"
                    aria-label="Show seasons"
                  >
                    <For each={show().seasons}>
                      {(season) => (
                        <li class="shrink-0">
                          <Button
                            type="button"
                            variant="outlined"
                            class={`rounded-full ${
                              activeSeason()?.id === season.id
                                ? 'border-secondary bg-secondary-container/45 text-on-secondary-container'
                                : ''
                            }`}
                            aria-pressed={activeSeason()?.id === season.id}
                            disabled={episodesLoading()}
                            onClick={() => void loadEpisodes(season)}
                          >
                            {seasonLabel(season)}
                          </Button>
                        </li>
                      )}
                    </For>
                  </ul>

                  <Suspense fallback={<SeasonEpisodesSkeleton />}>
                    <Show
                      when={hasSeasonEpisodes()}
                      fallback={
                        episodesLoading() ? (
                          <SeasonEpisodesSkeleton />
                        ) : (
                          <LibraryStatusPanel
                            title={episodesStatusTitle()}
                            description={episodesStatusDescription()}
                          />
                        )
                      }
                    >
                      <section class="space-y-3" aria-labelledby="season-episodes-title">
                        <h3
                          id="season-episodes-title"
                          class="text-on-surface text-[16px] leading-[24px] font-semibold"
                        >
                          {activeSeason() ? `${activeSeason()?.name} Episodes` : 'Episodes'}
                        </h3>
                        <div class="flex animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] flex-col gap-3">
                          <For each={seasonEpisodes()}>
                            {(episode) => (
                              <Card
                                variant="filled"
                                surfaceTint={false}
                                class="grid grid-cols-1 items-center gap-4 !p-3 sm:grid-cols-[160px_1fr_auto]"
                              >
                                {/* Episode thumbnail - landscape, episode-specific */}
                                <div class="bg-surface-container-lowest/60 aspect-video w-full overflow-hidden rounded-lg">
                                  <Show
                                    when={episode.artworkUrl}
                                    fallback={
                                      <div class="text-on-surface-variant flex h-full items-center justify-center text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
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

                                {/* Episode metadata column */}
                                <div class="min-w-0 space-y-1.5">
                                  <div class="flex flex-wrap items-center gap-2">
                                    <span class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                                      {episodeLabel(episode)}
                                    </span>
                                    <Show when={episode.played}>
                                      <StatusBadge variant="success">Played</StatusBadge>
                                    </Show>
                                    <Show when={episode.favorite}>
                                      <StatusBadge variant="success">Favorite</StatusBadge>
                                    </Show>
                                    <Show when={formatRuntime(episode.runtimeSeconds)}>
                                      {(runtime) => (
                                        <>
                                          <span class="text-on-surface-variant/70 text-[12px] leading-[16px]">
                                            {runtime()}
                                          </span>
                                        </>
                                      )}
                                    </Show>
                                    <Show
                                      when={
                                        episode.resumePositionSeconds != null &&
                                        episode.resumePositionSeconds > 0 &&
                                        !episode.played
                                      }
                                    >
                                      <span class="text-on-surface-variant/70">·</span>{' '}
                                      <span class="text-secondary text-[12px] leading-[16px] font-semibold">
                                        {Math.round(episode.playedPercentage ?? 0)}% watched
                                      </span>
                                    </Show>
                                  </div>
                                  <a
                                    href={`/library/items/${episode.id}`}
                                    class="text-on-surface block truncate text-[16px] leading-[24px] font-semibold hover:underline"
                                  >
                                    {episode.name}
                                  </a>
                                </div>

                                {/* Episode Action Column on the far right */}
                                <div class="flex shrink-0">
                                  <Show
                                    when={
                                      episode.resumePositionSeconds != null &&
                                      episode.resumePositionSeconds > 0 &&
                                      !episode.played
                                    }
                                    fallback={
                                      <Button
                                        type="button"
                                        variant="primary"
                                        class="rounded-full px-5 py-2 text-[14px] leading-[20px] font-semibold tracking-wide uppercase"
                                        disabled={episodePlayBusy() !== null || confirmBusy()}
                                        onClick={() => void playEpisode(episode)}
                                      >
                                        {episodePlayBusy() === episode.id ? 'Loading...' : 'Play'}
                                      </Button>
                                    }
                                  >
                                    <Button
                                      type="button"
                                      variant="primary"
                                      class="rounded-full px-5 py-2 text-[14px] leading-[20px] font-semibold tracking-wide uppercase"
                                      disabled={episodePlayBusy() !== null || confirmBusy()}
                                      onClick={() => void playEpisode(episode)}
                                    >
                                      {episodePlayBusy() === episode.id ? 'Loading...' : 'Resume'}
                                    </Button>
                                  </Show>
                                </div>
                              </Card>
                            )}
                          </For>
                        </div>
                      </section>
                    </Show>
                  </Suspense>
                </Show>
              </section>

              {/* Right Column (Sidebar): Series Info */}
              <aside class="order-1 space-y-6 lg:order-2">
                <Card as="article" variant="filled" class="space-y-4">
                  <div class="bg-surface-container-lowest/60 border-outline-variant aspect-[2/3] overflow-hidden rounded-2xl border">
                    <Show
                      when={show().artworkUrl}
                      fallback={
                        <div class="text-on-surface-variant flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
                          <Tv class="h-8 w-8" />
                          <p class="text-on-surface text-[16px] leading-[24px] font-semibold">
                            {show().name}
                          </p>
                          <p class="text-on-surface-variant/90 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                            No artwork
                          </p>
                        </div>
                      }
                    >
                      {(artworkUrl) => (
                        <img
                          src={artworkUrl()}
                          alt={`${show().name} artwork`}
                          class="h-full w-full animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] object-cover"
                        />
                      )}
                    </Show>
                  </div>
                  <div>
                    <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                      Series
                    </p>
                    <h1 class="font-display text-[28px] leading-[36px] font-bold tracking-tight">
                      {show().name}
                    </h1>
                    <p class="text-on-surface-variant mt-1 text-[14px] leading-[20px]">
                      {showSubtitle(show())}
                    </p>
                  </div>
                  <div class="flex flex-wrap gap-2">
                    <StatusBadge variant={show().played ? 'success' : 'neutral'}>
                      {show().played ? 'Played' : 'Unplayed'}
                    </StatusBadge>
                    <StatusBadge variant={show().favorite ? 'success' : 'neutral'}>
                      {show().favorite ? 'Favorite' : 'Not favorite'}
                    </StatusBadge>
                  </div>

                  <UserDataControls
                    itemId={show().id}
                    played={show().played}
                    favorite={show().favorite}
                    subject="show"
                    onUpdate={updateLibraryUserData}
                    onSuccess={reloadShow}
                  />

                  <Show when={show().overview}>
                    {(overview) => (
                      <p class="text-on-surface-variant border-outline-variant/30 border-t pt-3 text-[14px] leading-[20px] leading-relaxed">
                        {overview()}
                      </p>
                    )}
                  </Show>

                  <Show when={show().genres.length > 0}>
                    <div class="border-outline-variant/30 flex flex-wrap gap-1.5 border-t pt-3">
                      <For each={show().genres}>
                        {(genre) => (
                          <span class="border-outline-variant text-on-surface-variant/90 rounded-full border px-2.5 py-0.5 text-[11px] font-bold">
                            {genre}
                          </span>
                        )}
                      </For>
                    </div>
                  </Show>

                  {/* Secondary Play next episode shortcut */}
                  <Show when={show().nextEpisode}>
                    {(nextEpisode) => (
                      <div class="border-outline-variant/30 flex flex-col gap-3 border-t pt-3">
                        <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                          Up Next
                        </p>
                        <Button
                          type="button"
                          variant="primary"
                          class="w-full rounded-full"
                          disabled={playBusy() || confirmBusy()}
                          onClick={() => void playShow()}
                        >
                          {playBusy() ? 'Loading...' : 'Play Next Episode'}
                        </Button>
                        <a
                          href={`/library/items/${nextEpisode().id}`}
                          class="text-on-surface-variant hover:text-secondary block truncate text-center text-[12px] leading-[16px] underline-offset-4 hover:underline"
                        >
                          Next: {nextEpisode().name}
                        </a>
                      </div>
                    )}
                  </Show>
                </Card>
              </aside>
            </ConsoleGrid>
          )}
        </Show>
      </Suspense>
      <Show when={pendingPlayback()}>
        {(pending) => (
          <LibraryPlaybackChooser
            pending={pending()}
            busy={confirmBusy()}
            onCancel={() => setPendingPlayback(null)}
            onConfirm={(selection) => void confirmPlayback(selection)}
          />
        )}
      </Show>
      <Show when={playError()}>
        {(message) => <p class="text-error text-[12px] leading-[16px]">{message()}</p>}
      </Show>
    </div>
  );
}

function ShowDetailSkeleton() {
  return (
    <ConsoleGrid aria-hidden={true}>
      <section class="order-2 min-w-0 space-y-4 lg:order-1">
        <div class="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
          <div class="bg-surface-container-high/80 h-7 w-32 animate-pulse rounded-md" />
          <div class="bg-surface-container-high/60 h-4 w-28 animate-pulse rounded" />
        </div>
        <div class="border-outline-variant bg-surface-container-low/70 flex gap-2 overflow-x-auto rounded-2xl border p-2">
          <For each={[0, 1, 2]}>
            {() => (
              <div class="bg-surface-container-high/70 h-10 w-24 shrink-0 animate-pulse rounded-full" />
            )}
          </For>
        </div>
        <SeasonEpisodesSkeleton />
      </section>

      <aside class="order-1 space-y-6 lg:order-2">
        <Card as="article" variant="filled" surfaceTint={false} class="space-y-4">
          <div class="bg-surface-container-lowest/60 border-outline-variant aspect-[2/3] animate-pulse rounded-2xl border" />
          <div class="space-y-3">
            <div class="bg-surface-container-high/60 h-3 w-16 animate-pulse rounded" />
            <div class="bg-surface-container-high/80 h-8 w-4/5 animate-pulse rounded-md" />
            <div class="bg-surface-container-high/60 h-4 w-2/3 animate-pulse rounded" />
          </div>
          <div class="flex flex-wrap gap-2">
            <div class="bg-surface-container-high/70 h-7 w-24 animate-pulse rounded-full" />
            <div class="bg-surface-container-high/60 h-7 w-28 animate-pulse rounded-full" />
          </div>
          <div class="flex flex-wrap gap-3">
            <div class="bg-surface-container-high/70 h-10 w-28 animate-pulse rounded-full" />
            <div class="bg-surface-container-high/60 h-10 w-32 animate-pulse rounded-full" />
          </div>
          <div class="border-outline-variant/30 space-y-2 border-t pt-3">
            <div class="bg-surface-container-high/60 h-4 w-full animate-pulse rounded" />
            <div class="bg-surface-container-high/60 h-4 w-10/12 animate-pulse rounded" />
            <div class="bg-surface-container-high/50 h-4 w-7/12 animate-pulse rounded" />
          </div>
          <div class="border-outline-variant/30 border-t pt-3">
            <div class="bg-primary-container/40 h-11 w-full animate-pulse rounded-full" />
          </div>
        </Card>
      </aside>
    </ConsoleGrid>
  );
}

function SeasonEpisodesSkeleton() {
  return (
    <section class="space-y-3" aria-hidden="true">
      <div class="bg-surface-container-high/70 h-6 w-44 animate-pulse rounded-md" />
      <div class="flex flex-col gap-3">
        <For each={[0, 1, 2]}>
          {() => (
            <Card
              variant="filled"
              surfaceTint={false}
              class="grid grid-cols-1 items-center gap-4 !p-3 sm:grid-cols-[160px_1fr_auto]"
            >
              <div class="bg-surface-container-lowest/60 aspect-video w-full animate-pulse rounded-lg" />
              <div class="min-w-0 space-y-2">
                <div class="flex flex-wrap items-center gap-2">
                  <div class="bg-surface-container-high/70 h-3 w-14 animate-pulse rounded" />
                  <div class="bg-surface-container-high/60 h-6 w-20 animate-pulse rounded-full" />
                </div>
                <div class="bg-surface-container-high/80 h-5 w-4/5 animate-pulse rounded" />
              </div>
              <div class="bg-primary-container/40 h-10 w-24 animate-pulse rounded-full" />
            </Card>
          )}
        </For>
      </div>
    </section>
  );
}
