import type {
  VideoLibraryItem,
  VideoLibraryPlayRequest,
  VideoSeason,
  VideoShowDetail,
  VideoUserDataUpdateRequest,
} from '@bindings';
import { useAuthenticatedBootstrap } from '@components/AuthenticatedBootstrap';
import {
  DetailEpisodeList,
  DetailEpisodeListSkeleton,
} from '@components/library/DetailEpisodeList';
import {
  DetailRecommendationError,
  DetailRecommendationShelf,
  DetailRecommendationShelfSkeleton,
} from '@components/library/DetailRecommendationShelf';
import { ProgressPlayButton } from '@components/library/ProgressPlayButton';
import {
  DetailHero,
  type DetailHeroModel,
  DetailHeroSkeleton,
  LibraryStatusPanel,
  UserDataControls,
  formatRuntime,
  seasonLabel,
} from '@components/library/shared';
import { JellyPilotSelect } from '@components/ui';
import type { JellyPilotSelectItem } from '@components/ui';
import { cx } from '@styled-system/css';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, useCanGoBack, useNavigate, useRouter } from '@tanstack/solid-router';
import { Exit, Option } from 'effect';
import { Play, RefreshCw } from 'lucide-solid';
import { For, Show, Suspense, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import {
  fetchSeasonEpisodes,
  fetchSimilarVideoItems,
  fetchVideoShowDetail,
  initialSeasonForShow,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import type { LibraryExit, SeasonEpisodesState } from '~effects/library';
import { isLibrarySessionKeyConnected, queryKeys, runExit } from '~effects/query';
import { detailPlaybackProgress } from '~utils/libraryDetail';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../detailRoute.styles';
import * as showStyles from './showDetail.styles';

export const Route = createFileRoute('/_authenticated/library/shows/$seriesId')({
  component: LibraryShowDetailRoute,
});

const RECOMMENDATION_TITLE = 'More like this';

function episodeLabel(episode: VideoLibraryItem): string {
  if (episode.seasonNumber != null && episode.episodeNumber != null) {
    return `S${episode.seasonNumber.toString().padStart(2, '0')}E${episode.episodeNumber.toString().padStart(2, '0')}`;
  }
  return 'Episode';
}

function buildShowHeroModel(show: VideoShowDetail): DetailHeroModel {
  return {
    itemId: show.id,
    name: show.name,
    itemType: 'Series',
    artworkImageId: show.artworkImageId,
    backdropImageId: show.backdropImageId,
    productionYear: show.productionYear,
    runtime: null,
    overview: show.overview,
    communityRating: show.metadata.communityRating,
    officialRating: show.metadata.officialRating,
    genres: show.genres,
    creators: show.metadata.creators,
    cast: show.metadata.cast,
    seriesName: null,
    seriesId: null,
    episodeCode: null,
  };
}

function LibraryShowDetailRoute() {
  const params = Route.useParams();
  const router = useRouter();
  const navigate = useNavigate();
  const canGoBack = useCanGoBack();
  const queryClient = useQueryClient();
  const bootstrap = useAuthenticatedBootstrap();
  const sessionKey = bootstrap.sessionKey;
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
  const [playError, setPlayError] = createSignal<string | null>(null);

  const closeDetail = () => {
    if (canGoBack()) {
      router.history.back();
      return;
    }

    void navigate({ to: AUTHENTICATED_HOME_ROUTE });
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

  // Playback is decided from summary state alone; no item-detail refetch.
  const playSummary = async (episode: VideoLibraryItem) => {
    const resume =
      episode.resumePositionSeconds != null && episode.resumePositionSeconds > 0 && !episode.played;
    const result = await playbackMutation.mutateAsync({
      audioStreamIndex: null,
      itemId: episode.id,
      mode: resume ? 'resume' : 'start',
      startPositionSeconds: resume ? episode.resumePositionSeconds : 0,
      subtitleStreamIndex: null,
    });
    setPlayError(
      Exit.match(result, {
        onFailure: (cause) => commandFailureMessage(cause, 'Could not start playback'),
        onSuccess: () => null,
      }),
    );
  };
  const playShow = async () => {
    const show = detail();
    if (!show?.nextEpisode || playBusy()) {
      return;
    }

    setPlayBusy(true);
    setPlayError(null);
    await playSummary(show.nextEpisode);
    setPlayBusy(false);
  };
  const playEpisode = async (episode: VideoLibraryItem) => {
    if (episodePlayBusy()) {
      return;
    }

    setEpisodePlayBusy(episode.id);
    setPlayError(null);
    await playSummary(episode);
    setEpisodePlayBusy(null);
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
  const playbackProgress = () => {
    const next = detail()?.nextEpisode;
    return next
      ? detailPlaybackProgress(
          next.runtimeSeconds,
          next.resumePositionSeconds,
          next.playedPercentage,
        )
      : null;
  };
  const progressRemainingLabel = () => {
    const progress = playbackProgress();
    return progress ? `${progress.minutesRemaining} min remaining` : null;
  };
  const seasonMeta = () => {
    const episodes = seasonEpisodes();
    if (episodes.length === 0) {
      return null;
    }
    const totalSeconds = episodes.reduce((sum, ep) => sum + (ep.runtimeSeconds ?? 0), 0);
    const total = totalSeconds > 0 ? formatRuntime(totalSeconds) : null;
    const count = `${episodes.length} ${episodes.length === 1 ? 'episode' : 'episodes'}`;
    return total ? `${count} · ${total} total` : count;
  };

  // Recommendations (deferred, independent failure domain).
  const similarQuery = createQuery(() => ({
    queryKey: queryKeys.librarySimilarVideo(sessionKey(), params().seriesId),
    enabled:
      isLibrarySessionKeyConnected(sessionKey()) &&
      showQuery.isSuccess &&
      showQuery.data !== undefined &&
      Exit.isSuccess(showQuery.data),
    queryFn: () => runExit(fetchSimilarVideoItems(params().seriesId)),
  }));
  const similarItems = () => {
    const current = similarQuery.data;
    return current && Exit.isSuccess(current) ? current.value : [];
  };
  const similarFailed = () => {
    const current = similarQuery.data;
    return Boolean(current && !Exit.isSuccess(current));
  };
  const similarErrorMessage = () => {
    const current = similarQuery.data;
    return current && !Exit.isSuccess(current)
      ? commandFailureMessage(current.cause, 'Could not load recommendations')
      : '';
  };

  const heroModel = createMemo(() => {
    const show = detail();
    return show ? buildShowHeroModel(show) : null;
  });

  return (
    <div class={styles.stack}>
      <Suspense fallback={<ShowDetailSkeleton />}>
        <Show
          when={heroModel()}
          fallback={
            <div class={styles.contentSection}>
              <LibraryStatusPanel title={statusTitle()} description={statusDescription()} />
            </div>
          }
        >
          {(model) => {
            const show = () => detail() as VideoShowDetail;
            return (
              <>
                <DetailHero
                  titleId="show-detail-title"
                  model={model()}
                  onBack={closeDetail}
                  actions={() => (
                    <>
                      <ProgressPlayButton
                        label={playBusy() ? 'Loading...' : playShowLabel()}
                        percent={playbackProgress()?.percent ?? null}
                        remainingLabel={progressRemainingLabel()}
                        disabled={!show().nextEpisode || playBusy()}
                        onClick={() => void playShow()}
                        leadingIcon={
                          <Show when={playBusy()} fallback={<Play class={styles.playIcon} />}>
                            <RefreshCw class={cx(styles.icon4, styles.spinner)} />
                          </Show>
                        }
                      />
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
                  )}
                />

                <div class={styles.contentSection}>
                  <Show
                    when={show().seasons.length > 0}
                    fallback={
                      <LibraryStatusPanel
                        title="No seasons available"
                        description="Jellyfin returned no seasons for this show."
                      />
                    }
                  >
                    <div class={showStyles.seasonBar}>
                      <SeasonSelector
                        seasons={show().seasons}
                        activeSeason={activeSeason()}
                        disabled={episodesLoading()}
                        onSelect={loadEpisodes}
                      />
                      <Show when={seasonMeta()}>
                        {(meta) => <p class={showStyles.seasonMeta}>{meta()}</p>}
                      </Show>
                    </div>

                    <Suspense fallback={<DetailEpisodeListSkeleton />}>
                      <Show
                        when={hasSeasonEpisodes()}
                        fallback={
                          episodesLoading() ? (
                            <DetailEpisodeListSkeleton />
                          ) : (
                            <LibraryStatusPanel
                              title={episodesStatusTitle()}
                              description={episodesStatusDescription()}
                            />
                          )
                        }
                      >
                        <DetailEpisodeList
                          episodes={seasonEpisodes()}
                          busyItemId={episodePlayBusy()}
                          disabled={episodePlayBusy() !== null}
                          onPlay={(episode) => void playEpisode(episode)}
                        />
                      </Show>
                    </Suspense>
                  </Show>
                </div>

                <Show when={similarQuery.isPending}>
                  <DetailRecommendationShelfSkeleton title={RECOMMENDATION_TITLE} />
                </Show>
                <Show when={similarFailed()}>
                  <DetailRecommendationError
                    title={RECOMMENDATION_TITLE}
                    message={similarErrorMessage()}
                    onRetry={() => void similarQuery.refetch()}
                  />
                </Show>
                <Show when={similarItems().length > 0}>
                  <DetailRecommendationShelf title={RECOMMENDATION_TITLE} items={similarItems()} />
                </Show>
              </>
            );
          }}
        </Show>
      </Suspense>
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
  const seasonItems = createMemo<JellyPilotSelectItem[]>(() =>
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
        <ul class={showStyles.seasonTabs} aria-label="Show seasons">
          <For each={props.seasons}>
            {(season) => (
              <li>
                <button
                  type="button"
                  class={showStyles.seasonTab}
                  aria-pressed={props.activeSeason?.id === season.id}
                  disabled={props.disabled}
                  onClick={() => props.onSelect(season)}
                >
                  {seasonLabel(season)}
                </button>
              </li>
            )}
          </For>
        </ul>
      }
    >
      <div class={showStyles.seasonSelectWrap}>
        <JellyPilotSelect
          label="Season"
          items={seasonItems()}
          disabled={props.disabled}
          value={props.activeSeason?.id ?? ''}
          size="compact"
          onValueChange={selectSeason}
        />
      </div>
    </Show>
  );
}

function ShowDetailSkeleton() {
  return (
    <div class={styles.page}>
      <DetailHeroSkeleton />
      <div class={styles.contentSection} aria-hidden="true">
        <div class={styles.skeletonBar} />
        <DetailEpisodeListSkeleton />
      </div>
    </div>
  );
}
