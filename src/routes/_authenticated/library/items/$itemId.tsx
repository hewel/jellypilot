import type {
  VideoItemDetail,
  VideoLibraryPlayMode,
  VideoLibraryPlayRequest,
  VideoUserDataUpdateRequest,
} from '@bindings';
import { LibraryStatusPanel, UserDataControls, formatRuntime } from '@components/library/shared';
import { Button, StatusBadge } from '@components/ui';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import {
  Link,
  createFileRoute,
  useCanGoBack,
  useNavigate,
  useRouter,
} from '@tanstack/solid-router';
import { Exit } from 'effect';
import { ChevronLeft, Film, Play, RotateCcw, Tv } from 'lucide-solid';
import { For, Show, createMemo, createSignal } from 'solid-js';
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
import { imageSource } from '~utils/imageSource';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../detailRoute.styles';

export const Route = createFileRoute('/_authenticated/library/items/$itemId')({
  component: LibraryItemDetailRoute,
});

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

            return (
              <div class={styles.page}>
                <button
                  type="button"
                  class={styles.backLink}
                  aria-label="Back"
                  onClick={closeDetail}
                >
                  <ChevronLeft class={styles.icon4} aria-hidden="true" />
                  Back to library
                </button>

                <section class={styles.hero} aria-labelledby="item-detail-title">
                  <div class={styles.heroInfo}>
                    <div class={styles.badgeRow}>
                      <span class={styles.typeBadge}>
                        <Show
                          when={isEpisode()}
                          fallback={<Film class={styles.icon4} aria-hidden="true" />}
                        >
                          <Tv class={styles.icon4} aria-hidden="true" />
                        </Show>
                        {item().itemType}
                      </span>
                      <Show when={item().productionYear}>
                        {(year) => <span class={styles.badge}>{year()}</span>}
                      </Show>
                      <For each={item().genres.slice(0, 3)}>
                        {(genre) => <span class={styles.badge}>{genre}</span>}
                      </For>
                      <StatusBadge variant={item().played ? 'success' : 'neutral'}>
                        {item().played ? 'Played' : 'Unplayed'}
                      </StatusBadge>
                      <StatusBadge variant={item().favorite ? 'success' : 'neutral'}>
                        {item().favorite ? 'Favorite' : 'Not favorite'}
                      </StatusBadge>
                    </div>

                    <h1 id="item-detail-title" class={styles.heroTitle}>
                      {item().name}
                    </h1>

                    <p class={styles.heroMeta}>
                      <Show when={isEpisode() && item().seriesName}>
                        <Show when={item().seriesId} fallback={<span>{item().seriesName}</span>}>
                          {(seriesId) => (
                            <Link
                              to="/library/shows/$seriesId"
                              params={{ seriesId: seriesId() }}
                              class={styles.heroMetaLink}
                            >
                              {item().seriesName}
                            </Link>
                          )}
                        </Show>
                        <span aria-hidden="true">·</span>
                        <span>{episodeCode()}</span>
                      </Show>
                      <Show when={!isEpisode() && item().productionYear !== null}>
                        <span>{item().productionYear}</span>
                      </Show>
                      <Show when={formatRuntime(item().runtimeSeconds)}>
                        {(runtime) => (
                          <>
                            <span aria-hidden="true">·</span>
                            <span>{runtime()}</span>
                          </>
                        )}
                      </Show>
                      <Show when={watchedPercent()}>
                        {(percent) => (
                          <>
                            <span aria-hidden="true">·</span>
                            <span>{percent()}% watched</span>
                          </>
                        )}
                      </Show>
                    </p>

                    <Show when={item().overview}>
                      {(overview) => <p class={styles.heroOverview}>{overview()}</p>}
                    </Show>

                    <div class={styles.accentBar} aria-hidden="true" />

                    <div class={styles.heroActions}>
                      <Show
                        when={item().canResume}
                        fallback={
                          <Button
                            type="button"
                            variant="primary"
                            class={styles.pillButton}
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
                          class={styles.pillButton}
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
                    </div>
                  </div>

                  <div class={styles.heroArt}>
                    <Show
                      when={item().artworkImageId}
                      fallback={
                        <div class={styles.heroArtFallback} aria-hidden="true">
                          {item().name.charAt(0)}
                        </div>
                      }
                    >
                      {(imageId) => (
                        <img
                          src={imageSource(imageId())}
                          alt={`${item().name} artwork`}
                          class={styles.heroArtImage}
                        />
                      )}
                    </Show>
                    <Show when={item().productionYear}>
                      {(year) => <span class={styles.heroArtYear}>{year()}</span>}
                    </Show>
                    <Show when={watchedPercent()}>
                      {(percent) => (
                        <div
                          class={styles.heroArtProgress}
                          role="progressbar"
                          aria-label={`${item().name} watch progress`}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-valuenow={percent()}
                        >
                          <div
                            class={styles.heroArtProgressBar}
                            style={{ width: `${percent()}%` }}
                          />
                        </div>
                      )}
                    </Show>
                  </div>
                </section>
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
      <div class={styles.skeletonHero} />
    </div>
  );
}
