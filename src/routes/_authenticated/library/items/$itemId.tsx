import type {
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryPlayMode,
  VideoLibraryPlayRequest,
  VideoPlaybackStreamOption,
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
import {
  DetailHero,
  type DetailHeroInfoRow,
  type DetailHeroModel,
  DetailHeroSkeleton,
  LibraryStatusPanel,
  UserDataControls,
  formatRuntime,
} from '@components/library/shared';
import { Button } from '@components/ui';
import { cx } from '@styled-system/css';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, useCanGoBack, useNavigate, useRouter } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Play } from 'lucide-solid';
import { Show, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import {
  fetchSeasonEpisodes,
  fetchSimilarVideoItems,
  fetchVideoItemDetail,
  fetchVideoItemStreams,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import { isLibrarySessionKeyConnected, queryKeys, runExit } from '~effects/query';
import { detailPlaybackProgress, neighboringEpisodes } from '~utils/libraryDetail';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../detailRoute.styles';

export const Route = createFileRoute('/_authenticated/library/items/$itemId')({
  component: LibraryItemDetailRoute,
});

const RECOMMENDATION_TITLE = 'More like this';

function streamLanguages(streams: VideoPlaybackStreamOption[]) {
  return [...new Set(streams.map((stream) => stream.language ?? stream.label))].join(', ');
}

function episodeCode(item: VideoItemDetail): string | null {
  const { seasonNumber, episodeNumber } = item;
  if (seasonNumber !== null && episodeNumber !== null) {
    return `S${seasonNumber.toString().padStart(2, '0')}E${episodeNumber.toString().padStart(2, '0')}`;
  }
  return null;
}

function buildHeroModel(item: VideoItemDetail): DetailHeroModel {
  const isEpisode = item.itemType === 'Episode';
  return {
    itemId: item.id,
    name: item.name,
    itemType: isEpisode ? 'Episode' : 'Movie',
    artworkImageId: item.artworkImageId,
    backdropImageId: item.backdropImageId,
    productionYear: item.productionYear,
    runtime: formatRuntime(item.runtimeSeconds),
    overview: item.overview,
    communityRating: item.metadata.communityRating,
    officialRating: item.metadata.officialRating,
    genres: item.genres,
    creators: item.metadata.creators,
    cast: item.metadata.cast,
    seriesName: isEpisode ? item.seriesName : null,
    seriesId: isEpisode ? item.seriesId : null,
    episodeCode: isEpisode ? episodeCode(item) : null,
    progress: detailPlaybackProgress(
      item.runtimeSeconds,
      item.resumePositionSeconds,
      item.playedPercentage,
    ),
  };
}

function LibraryItemDetailRoute() {
  const params = Route.useParams();
  const router = useRouter();
  const navigate = useNavigate();
  const canGoBack = useCanGoBack();
  const queryClient = useQueryClient();
  const bootstrap = useAuthenticatedBootstrap();
  const sessionKey = bootstrap.sessionKey;
  const detailQuery = createQuery(() => ({
    queryKey: queryKeys.libraryItemDetail(sessionKey(), params().itemId),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchVideoItemDetail(params().itemId)),
  }));
  const streamsQuery = createQuery(() => ({
    queryKey: queryKeys.libraryItemStreams(sessionKey(), params().itemId),
    enabled:
      isLibrarySessionKeyConnected(sessionKey()) &&
      detailQuery.isSuccess &&
      detailQuery.data !== undefined &&
      Exit.isSuccess(detailQuery.data),
    queryFn: () => runExit(fetchVideoItemStreams(params().itemId)),
  }));
  const similarQuery = createQuery(() => ({
    queryKey: queryKeys.librarySimilarVideo(sessionKey(), params().itemId),
    enabled:
      isLibrarySessionKeyConnected(sessionKey()) &&
      detailQuery.isSuccess &&
      detailQuery.data !== undefined &&
      Exit.isSuccess(detailQuery.data),
    queryFn: () => runExit(fetchSimilarVideoItems(params().itemId)),
  }));
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
  const detailResult = () => (detailQuery.isFetched ? detailQuery.data : undefined);
  const detail = () => {
    const current = detailResult();
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const itemStreams = () => {
    if (!streamsQuery.isSuccess) {
      return null;
    }
    const current = streamsQuery.data;
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const detailPending = () =>
    !bootstrap.connected() || (isLibrarySessionKeyConnected(sessionKey()) && detailQuery.isPending);
  const statusTitle = () => {
    const current = detailResult();
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load item detail';
    }
    return 'Loading item detail';
  };
  const statusDescription = () => {
    const current = detailResult();
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load item detail');
    }
    return 'JellyPilot is loading Movie or Episode detail data from Jellyfin.';
  };
  const technicalRows = (): DetailHeroInfoRow[] => {
    const streams = itemStreams();
    if (!streams) {
      return [];
    }
    const rows: DetailHeroInfoRow[] = [];
    const audio = streamLanguages(streams.audioStreams);
    if (audio) {
      rows.push({ label: 'Audio', value: audio });
    }
    const subtitles = streamLanguages(streams.subtitleStreams);
    if (subtitles) {
      rows.push({ label: 'Subtitles', value: subtitles });
    }
    return rows;
  };
  const technicalRowsLoading = () => streamsQuery.isPending || streamsQuery.isFetching;
  const startPlayback = async (item: VideoItemDetail, mode: VideoLibraryPlayMode) => {
    if (!item.canPlay || playBusy()) {
      return;
    }

    setPlayBusy(true);
    setPlayError(null);
    const result = await playbackMutation.mutateAsync({
      audioStreamIndex: null,
      itemId: item.id,
      mode,
      startPositionSeconds: mode === 'resume' ? item.resumePositionSeconds : 0,
      subtitleStreamIndex: null,
    });
    const message = Exit.match(result, {
      onFailure: (cause) => commandFailureMessage(cause, 'Could not start playback'),
      onSuccess: () => null,
    });
    setPlayError(message);
    setPlayBusy(false);
  };

  // Episode-only "More from Season N": resolve neighbors from the exact season.
  const seasonRequest = () => {
    const item = detail();
    if (
      !item ||
      item.itemType !== 'Episode' ||
      item.seriesId === null ||
      item.seasonNumber === null
    ) {
      return null;
    }
    return { seriesId: item.seriesId, seasonNumber: item.seasonNumber };
  };
  const seasonEpisodesQuery = createQuery(() => {
    const request = seasonRequest();
    return {
      queryKey: queryKeys.librarySeasonEpisodes(
        sessionKey(),
        request?.seriesId ?? 'none',
        request ? `season-${request.seasonNumber}` : 'none',
      ),
      enabled: request !== null && isLibrarySessionKeyConnected(sessionKey()),
      queryFn: () =>
        request
          ? runExit(
              fetchSeasonEpisodes({
                seasonId: null,
                seasonNumber: request.seasonNumber,
                seriesId: request.seriesId,
              }),
            )
          : Promise.resolve(null),
    };
  });
  const neighborEpisodes = () => {
    const current = seasonEpisodesQuery.data;
    const item = detail();
    if (!current || !Exit.isSuccess(current) || !item) {
      return [];
    }
    return neighboringEpisodes(current.value.page.episodes, item.id);
  };
  const showSeasonSection = () =>
    seasonRequest() !== null && (seasonEpisodesQuery.isPending || neighborEpisodes().length > 0);
  const playNeighborEpisode = async (episode: VideoLibraryItem) => {
    if (episodePlayBusy()) {
      return;
    }

    const resume =
      episode.resumePositionSeconds !== null &&
      episode.resumePositionSeconds > 0 &&
      !episode.played;
    setEpisodePlayBusy(episode.id);
    setPlayError(null);
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
    setEpisodePlayBusy(null);
  };

  // Recommendations (deferred, independent failure domain).
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
    const item = detail();
    return item ? buildHeroModel(item) : null;
  });

  return (
    <div class={styles.stack}>
      <Show when={!detailPending()} fallback={<ItemDetailSkeleton />}>
        <Show
          when={heroModel()}
          fallback={
            <div class={styles.contentSection}>
              <LibraryStatusPanel title={statusTitle()} description={statusDescription()} />
            </div>
          }
        >
          {(model) => {
            const item = () => detail() as VideoItemDetail;
            return (
              <>
                <DetailHero
                  titleId="item-detail-title"
                  model={model()}
                  technicalRows={technicalRows()}
                  technicalRowsLoading={technicalRowsLoading()}
                  onBack={closeDetail}
                  actions={() => (
                    <>
                      <Show
                        when={item().canResume}
                        fallback={
                          <Button
                            type="button"
                            variant="primary"
                            class={cx(styles.pillButton, styles.playGlow)}
                            disabled={!item().canPlay || playBusy()}
                            onClick={() => void startPlayback(item(), 'start')}
                            leadingIcon={<Play class={styles.playIcon} />}
                          >
                            Play
                          </Button>
                        }
                      >
                        <Button
                          type="button"
                          variant="primary"
                          class={cx(styles.pillButton, styles.playGlow)}
                          disabled={!item().canPlay || playBusy()}
                          onClick={() => void startPlayback(item(), 'resume')}
                          leadingIcon={<Play class={styles.playIcon} />}
                        >
                          Resume
                        </Button>
                      </Show>
                      <UserDataControls
                        itemId={item().id}
                        played={item().played}
                        favorite={item().favorite}
                        subject={item().itemType.toLowerCase()}
                        playFromBeginning={
                          item().canResume
                            ? {
                                disabled: !item().canPlay || playBusy(),
                                onSelect: () => void startPlayback(item(), 'start'),
                              }
                            : undefined
                        }
                        onUpdate={(request) => userDataMutation.mutateAsync(request)}
                        onSuccess={() => {
                          const itemType = item().itemType;
                          queryClient.invalidateQueries({
                            queryKey: queryKeys.libraryItemDetail(sessionKey(), params().itemId),
                          });
                          queryClient.invalidateQueries({
                            queryKey: queryKeys.libraryMediaDetail(
                              sessionKey(),
                              itemType,
                              params().itemId,
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

                <Show when={showSeasonSection()}>
                  <section class={styles.contentSection} aria-label="More from this season">
                    <h2 class={styles.sectionHeading}>
                      {`More from Season ${item().seasonNumber ?? ''}`}
                    </h2>
                    <Show
                      when={neighborEpisodes().length > 0}
                      fallback={<DetailEpisodeListSkeleton rows={2} />}
                    >
                      <DetailEpisodeList
                        episodes={neighborEpisodes()}
                        busyItemId={episodePlayBusy()}
                        disabled={episodePlayBusy() !== null}
                        onPlay={(episode) => void playNeighborEpisode(episode)}
                      />
                    </Show>
                  </section>
                </Show>

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
      </Show>
      <Show when={playError()}>{(message) => <p class={styles.error}>{message()}</p>}</Show>
    </div>
  );
}

function ItemDetailSkeleton() {
  return (
    <div class={styles.page}>
      <DetailHeroSkeleton />
    </div>
  );
}
