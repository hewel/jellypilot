import type {
  VideoLibraryItem,
  VideoLibraryPlayRequest,
  VideoSeason,
  VideoUserDataUpdateRequest,
} from '@bindings';
import { DetailHero } from '@components/library/DetailHero';
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
import { Badge, Button, Card, Link, Selector } from '@jellypilot/ui';
import type { SelectorItem } from '@jellypilot/ui';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, useCanGoBack, useNavigate, useRouter } from '@tanstack/solid-router';
import { Exit, Option } from 'effect';
import { Film, Play, RefreshCw, Tv } from 'lucide-solid';
import { For, Show, Suspense, createEffect, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import {
  fetchSeasonEpisodes,
  fetchVideoItemDetail,
  fetchVideoShowDetail,
  initialSeasonForShow,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import type { LibraryExit, SeasonEpisodesState } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';
import { imageSource } from '~utils/imageSource';

import * as styles from '../detailRoute.css';

export const Route = createFileRoute('/_authenticated/library/shows/$seriesId')({
  component: LibraryShowDetailRoute,
});

function LibraryShowDetailRoute() {
  const params = Route.useParams();
  const router = useRouter();
  const navigate = useNavigate();
  const canGoBack = useCanGoBack();
  const queryClient = useQueryClient();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState()),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const showQuery = createQuery(() => ({
    queryKey: queryKeys.libraryShowDetail(sessionKey(), params().seriesId),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchVideoShowDetail(params().seriesId)),
  }));
  const [selectedSeason, setSelectedSeason] = createSignal<VideoSeason | null>(null);
  const playbackMutation = createMutation(() => ({
    mutationFn: (request: VideoLibraryPlayRequest) => runExit(startLibraryPlayback(request)),
  }));
  const userDataMutation = createMutation(() => ({
    mutationFn: (request: VideoUserDataUpdateRequest) => runExit(updateLibraryUserData(request)),
  }));
  const [playBusy, setPlayBusy] = createSignal(false);
  const [episodePlayBusy, setEpisodePlayBusy] = createSignal<string | null>(null);
  const [confirmBusy, setConfirmBusy] = createSignal(false);
  const [pendingPlayback, setPendingPlayback] = createSignal<PendingLibraryPlayback | null>(null);
  const [playError, setPlayError] = createSignal<string | null>(null);

  const closeDetail = () => {
    if (canGoBack()) {
      router.history.back();
      return;
    }

    void navigate({ to: '/library' });
  };
  const detail = () =>
    showQuery.data && Exit.isSuccess(showQuery.data) ? showQuery.data.value : null;
  const activeSeason = () => {
    const selected = selectedSeason();
    if (selected) {
      return selected;
    }

    return Option.fromNullishOr(detail()).pipe(
      Option.flatMap((show) => initialSeasonForShow(show)),
      Option.getOrNull,
    );
  };
  const seasonEpisodesQuery = createQuery<LibraryExit<SeasonEpisodesState> | null>(() => {
    const season = activeSeason();
    return {
      queryKey: queryKeys.librarySeasonEpisodes(
        sessionKey(),
        params().seriesId,
        season?.id ?? 'none',
      ),
      enabled: season !== null && isLibrarySessionKeyConnected(sessionKey()),
      queryFn: () => {
        if (!season) {
          return Promise.resolve(null);
        }
        return runExit(
          fetchSeasonEpisodes({
            seasonId: season.id,
            seasonNumber: season.seasonNumber,
            seriesId: params().seriesId,
          }),
        );
      },
    };
  });
  const currentEpisodes = () => seasonEpisodesQuery.data;
  const seasonEpisodes = () => {
    const current = currentEpisodes();
    return current && Exit.isSuccess(current) ? current.value.page.episodes : [];
  };
  const hasSeasonEpisodes = () => seasonEpisodes().length > 0;
  const episodesLoading = () => seasonEpisodesQuery.isPending || seasonEpisodesQuery.isFetching;
  const loadEpisodes = (season: VideoSeason) => {
    setSelectedSeason(season);
  };
  const openEpisodePlaybackChooser = async (itemId: string) => {
    const result = await queryClient.fetchQuery({
      queryKey: queryKeys.libraryItemDetail(sessionKey(), itemId),
      queryFn: () => runExit(fetchVideoItemDetail(itemId)),
    });
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
    const result = await playbackMutation.mutateAsync({
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
    const current = showQuery.data;
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load show detail';
    }
    return 'Loading show detail';
  };
  const statusDescription = () => {
    const current = showQuery.data;
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load show detail');
    }
    return 'JellyPilot is loading Show detail, seasons, and Jellyfin next-up data.';
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
      return 'JellyPilot is loading exact Episode cards for the selected Season.';
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
  const playShowLabel = () => {
    const show = detail();
    const nextEpisode = show?.nextEpisode;
    if (!nextEpisode) {
      return 'Play';
    }
    const prefix =
      nextEpisode.resumePositionSeconds != null &&
      nextEpisode.resumePositionSeconds > 0 &&
      !nextEpisode.played
        ? 'Continue'
        : 'Play';
    return `${prefix} ${episodeLabel(nextEpisode)}`;
  };

  return (
    <div class={styles.stack}>
      <Suspense fallback={<ShowDetailSkeleton />}>
        <Show
          when={detail()}
          fallback={<LibraryStatusPanel title={statusTitle()} description={statusDescription()} />}
        >
          {(show) => (
            <>
              <DetailHero
                title={show().name}
                subtitle={showSubtitle(show())}
                backdropImageId={show().backdropImageId ?? null}
                artworkImageId={show().artworkImageId ?? null}
                artworkAspect="poster"
                typeLabel="Series"
                typeIcon={<Tv class={styles.icon6} />}
                onBack={closeDetail}
                badges={
                  <>
                    <Badge tone={show().played ? 'success' : 'neutral'}>
                      {show().played ? 'Played' : 'Unplayed'}
                    </Badge>
                    <Badge tone={show().favorite ? 'success' : 'neutral'}>
                      {show().favorite ? 'Favorite' : 'Not favorite'}
                    </Badge>
                  </>
                }
                actions={
                  <>
                    <Button
                      type="button"
                      variant="primary"
                      class={styles.pillButton}
                      disabled={!show().nextEpisode || playBusy() || confirmBusy()}
                      onClick={() => void playShow()}
                    >
                      <Show when={playBusy()} fallback={<Play class={styles.playIcon} />}>
                        <RefreshCw class={`${styles.icon4} ${styles.spinner}`} />
                      </Show>
                      {playBusy() ? 'Loading...' : playShowLabel()}
                    </Button>
                    <UserDataControls
                      itemId={show().id}
                      played={show().played}
                      favorite={show().favorite}
                      subject="show"
                      onUpdate={(request) => userDataMutation.mutateAsync(request)}
                      onSuccess={() => {
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryShowDetail(sessionKey(), params().seriesId),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryMediaDetail(
                            sessionKey(),
                            'Series',
                            params().seriesId,
                          ),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.librarySeasonEpisodesRoot(
                            sessionKey(),
                            params().seriesId,
                          ),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryHome(sessionKey()),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryBrowseRoot(sessionKey()),
                        });
                      }}
                    />
                  </>
                }
              />

              <div class={styles.content}>
                <Show when={show().overview}>
                  {(overview) => <p class={styles.overview}>{overview()}</p>}
                </Show>

                <Show when={show().genres.length > 0}>
                  <div class={styles.pillRow}>
                    <For each={show().genres}>
                      {(genre) => <span class={styles.genre}>{genre}</span>}
                    </For>
                  </div>
                </Show>

                <section class={styles.section} aria-labelledby="show-seasons-title">
                  <div class={styles.sectionHeader}>
                    <div>
                      <h2 id="show-seasons-title" class={styles.sectionTitle}>
                        Episodes
                      </h2>
                    </div>
                    <p class={styles.sectionSubtitle}>{show().seasons.length} seasons available</p>
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
                    <SeasonSelector
                      seasons={show().seasons}
                      activeSeason={activeSeason()}
                      disabled={episodesLoading()}
                      onSelect={loadEpisodes}
                    />

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
                        <section
                          class={styles.sectionCompact}
                          aria-labelledby="season-episodes-title"
                        >
                          <h3 id="season-episodes-title" class={styles.titleSmall}>
                            {activeSeason() ? `${activeSeason()?.name} Episodes` : 'Episodes'}
                          </h3>
                          <div class={styles.fadeList}>
                            <For each={seasonEpisodes()}>
                              {(episode) => (
                                <EpisodeRow
                                  episode={episode}
                                  label={episodeLabel(episode)}
                                  busy={episodePlayBusy() === episode.id}
                                  disabled={episodePlayBusy() !== null || confirmBusy()}
                                  onPlay={() => void playEpisode(episode)}
                                />
                              )}
                            </For>
                          </div>
                        </section>
                      </Show>
                    </Suspense>
                  </Show>
                </section>
              </div>
            </>
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
      <Show when={playError()}>{(message) => <p class={styles.error}>{message()}</p>}</Show>
    </div>
  );
}

function SeasonSelector(props: {
  seasons: VideoSeason[];
  activeSeason: VideoSeason | null;
  disabled: boolean;
  onSelect: (season: VideoSeason) => void;
}) {
  const seasonItems = createMemo<SelectorItem[]>(() =>
    props.seasons.map((season) => ({
      label: seasonLabel(season),
      value: season.id,
    })),
  );
  const selectSeason = (seasonId: string) => {
    const season = props.seasons.find((item) => item.id === seasonId);
    if (season) {
      props.onSelect(season);
    }
  };

  return (
    <Show
      when={props.seasons.length > 6}
      fallback={
        <ul class={styles.seasonTabs} aria-label="Show seasons">
          <For each={props.seasons}>
            {(season) => (
              <li class={styles.seasonItem}>
                <Button
                  type="button"
                  variant="outline"
                  class={styles.pillButton}
                  classList={{ [styles.selectedSeason]: props.activeSeason?.id === season.id }}
                  aria-pressed={props.activeSeason?.id === season.id}
                  disabled={props.disabled}
                  onClick={() => props.onSelect(season)}
                >
                  {seasonLabel(season)}
                </Button>
              </li>
            )}
          </For>
        </ul>
      }
    >
      <div class={styles.selectWrap}>
        <span>Season</span>
        <Selector
          aria-label="Season"
          items={seasonItems()}
          disabled={props.disabled}
          value={props.activeSeason?.id ?? null}
          onValueChange={(seasonId) => {
            if (seasonId) selectSeason(seasonId);
          }}
        />
      </div>
    </Show>
  );
}

function EpisodeRow(props: {
  episode: VideoLibraryItem;
  label: string;
  busy: boolean;
  disabled: boolean;
  onPlay: () => void;
}) {
  const [imageFailed, setImageFailed] = createSignal(false);
  const hasResume = () =>
    props.episode.resumePositionSeconds != null &&
    props.episode.resumePositionSeconds > 0 &&
    !props.episode.played;

  createEffect(() => {
    props.episode.artworkImageId;
    setImageFailed(false);
  });

  return (
    <Card variant="filled" class={styles.episodeCard}>
      <div class={styles.episodeImageWrap}>
        <Show
          when={!imageFailed() ? props.episode.artworkImageId : null}
          fallback={
            <div class={styles.episodeFallback}>
              <Film class={styles.icon4} />
            </div>
          }
        >
          {(imageId) => (
            <img
              src={imageSource(imageId())}
              alt={`${props.episode.name} artwork`}
              class={styles.image}
              loading="lazy"
              onError={() => setImageFailed(true)}
            />
          )}
        </Show>
      </div>

      <div class={styles.episodeCopy}>
        <div class={styles.episodeMeta}>
          <span class={styles.episodeLabel}>{props.label}</span>
          <Show when={props.episode.played}>
            <Badge tone="success">Played</Badge>
          </Show>
          <Show when={props.episode.favorite}>
            <Badge tone="success">Favorite</Badge>
          </Show>
          <Show when={formatRuntime(props.episode.runtimeSeconds)}>
            {(runtime) => <span class={styles.muted}>{runtime()}</span>}
          </Show>
          <Show when={hasResume()}>
            <span class={styles.muted}>·</span>
            <span class={styles.progressText}>
              {Math.round(props.episode.playedPercentage ?? 0)}% watched
            </span>
          </Show>
        </div>
        <Link href={`/library/items/${props.episode.id}`} class={styles.episodeLink}>
          {props.episode.name}
        </Link>
      </div>

      <div class={styles.actionCell}>
        <Button
          type="button"
          variant="primary"
          class={styles.episodeButton}
          disabled={props.disabled}
          onClick={props.onPlay}
        >
          <Show when={props.busy} fallback={<Play class={styles.playIcon} />}>
            <RefreshCw class={`${styles.icon4} ${styles.spinner}`} />
          </Show>
          {props.busy ? 'Loading...' : hasResume() ? 'Resume' : 'Play'}
        </Button>
      </div>
    </Card>
  );
}

function ShowDetailSkeleton() {
  return (
    <article class={styles.stack} aria-hidden="true">
      <div class={styles.skeletonHero} />
      <div class={styles.skeletonContent}>
        <div class={styles.sectionCompact}>
          <div class={styles.skeletonLine} />
          <div class={`${styles.skeletonLine} ${styles.skeletonLineShort}`} />
        </div>
        <div class={styles.pillRow}>
          <For each={[0, 1, 2]}>{() => <div class={styles.skeletonPill} />}</For>
        </div>
        <div class={styles.skeletonTitle} />
        <div class={styles.seasonTabs}>
          <For each={[0, 1, 2]}>{() => <div class={styles.skeletonPill} />}</For>
        </div>
        <SeasonEpisodesSkeleton />
      </div>
    </article>
  );
}

function SeasonEpisodesSkeleton() {
  return (
    <section class={styles.sectionCompact} aria-hidden="true">
      <div class={styles.skeletonTitle} />
      <div class={styles.fadeList}>
        <For each={[0, 1, 2]}>
          {() => (
            <Card variant="filled" class={styles.episodeCard}>
              <div class={styles.skeletonEpisodeImage} />
              <div class={styles.episodeCopy}>
                <div class={styles.episodeMeta}>
                  <div class={styles.skeletonMiniLine} />
                  <div class={styles.skeletonSmallPill} />
                </div>
                <div class={styles.skeletonEpisodeTitle} />
              </div>
              <div class={styles.skeletonButton} />
            </Card>
          )}
        </For>
      </div>
    </section>
  );
}
