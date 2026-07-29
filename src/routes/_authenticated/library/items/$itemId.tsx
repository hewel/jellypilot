import type {
  VideoItemDetail,
  VideoLibraryPlayMode,
  VideoLibraryPlayRequest,
  VideoPlaybackStreamOption,
  VideoUserDataUpdateRequest,
} from '@bindings';
import {
  DetailHero,
  type DetailHeroInfoRow,
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
import { Film, Play, RotateCcw, Tv } from 'lucide-solid';
import { Show, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import {
  fetchVideoItemDetail,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../detailRoute.styles';

export const Route = createFileRoute('/_authenticated/library/items/$itemId')({
  component: LibraryItemDetailRoute,
});

function streamLanguages(streams: VideoPlaybackStreamOption[]) {
  return [...new Set(streams.map((stream) => stream.language ?? stream.label))].join(', ');
}

function LibraryItemDetailRoute() {
  const params = Route.useParams();
  const router = useRouter();
  const navigate = useNavigate();
  const canGoBack = useCanGoBack();
  const queryClient = useQueryClient();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const detailQuery = createQuery(() => ({
    queryKey: queryKeys.libraryItemDetail(sessionKey(), params().itemId),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchVideoItemDetail(params().itemId)),
  }));
  const playbackMutation = createMutation(() => ({
    mutationFn: (request: VideoLibraryPlayRequest) => runExit(startLibraryPlayback(request)),
  }));
  const userDataMutation = createMutation(() => ({
    mutationFn: (request: VideoUserDataUpdateRequest) => runExit(updateLibraryUserData(request)),
  }));
  const [playBusy, setPlayBusy] = createSignal(false);
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

  return (
    <div class={styles.stack}>
      <Show
        when={!(connectionQuery.isPending || (detailQuery.isFetching && !detailQuery.isFetched))}
        fallback={<ItemDetailSkeleton />}
      >
        <Show
          when={detail()}
          fallback={<LibraryStatusPanel title={statusTitle()} description={statusDescription()} />}
        >
          {(item) => {
            const isEpisode = () => item().itemType === 'Episode';
            const episodeCode = () => {
              const { seasonNumber, episodeNumber } = item();
              return seasonNumber !== null && episodeNumber !== null
                ? `S${seasonNumber.toString().padStart(2, '0')}E${episodeNumber.toString().padStart(2, '0')}`
                : 'Episode';
            };
            const watchedPercent = () => {
              const { canResume, playedPercentage } = item();
              return canResume && playedPercentage !== null ? Math.round(playedPercentage) : null;
            };
            const infoRows = (): DetailHeroInfoRow[] => {
              const rows: DetailHeroInfoRow[] = [{ label: 'Type', value: item().itemType }];
              if (isEpisode() && item().seriesName) {
                rows.push({ label: 'Series', value: item().seriesName ?? '' });
              }
              const audio = streamLanguages(item().audioStreams);
              if (audio) {
                rows.push({ label: 'Audio', value: audio });
              }
              const subtitles = streamLanguages(item().subtitleStreams);
              if (subtitles) {
                rows.push({ label: 'Subtitles', value: subtitles });
              }
              return rows;
            };

            return (
              <div class={styles.page}>
                <DetailHero
                  titleId="item-detail-title"
                  name={item().name}
                  typeLabel={item().itemType}
                  typeIcon={
                    <Show
                      when={isEpisode()}
                      fallback={<Film class={styles.icon4} aria-hidden="true" />}
                    >
                      <Tv class={styles.icon4} aria-hidden="true" />
                    </Show>
                  }
                  imageId={item().backdropImageId ?? item().artworkImageId}
                  year={item().productionYear}
                  runtime={formatRuntime(item().runtimeSeconds)}
                  watchedPercent={watchedPercent()}
                  played={item().played}
                  favorite={item().favorite}
                  genres={item().genres}
                  overview={item().overview}
                  infoRows={infoRows()}
                  seriesName={isEpisode() ? item().seriesName : null}
                  seriesId={isEpisode() ? item().seriesId : null}
                  episodeCode={isEpisode() ? episodeCode() : null}
                  onBack={closeDetail}
                  actions={
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
                        <Button
                          type="button"
                          variant="secondary"
                          class={styles.pillButton}
                          disabled={!item().canPlay || playBusy()}
                          onClick={() => void startPlayback(item(), 'start')}
                          leadingIcon={<RotateCcw class={styles.icon4} />}
                        >
                          Play from beginning
                        </Button>
                      </Show>
                      <UserDataControls
                        itemId={item().id}
                        played={item().played}
                        favorite={item().favorite}
                        subject={item().itemType.toLowerCase()}
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
                  }
                />
              </div>
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
    <div class={styles.page} aria-hidden="true">
      <DetailHeroSkeleton />
    </div>
  );
}
