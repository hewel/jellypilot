import type {
  VideoItemDetail,
  VideoLibraryPlayMode,
  VideoLibraryPlayRequest,
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
  detailSubtitleElement,
  formatRuntime,
} from '@components/library/shared';
import { Button, StatusBadge } from '@components/ui';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { createFileRoute, useCanGoBack, useNavigate, useRouter } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Film, Play, RotateCcw, Tv } from 'lucide-solid';
import { For, Show, Suspense, createMemo, createSignal } from 'solid-js';
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

import * as styles from '../detailRoute.css';

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
    queryFn: () => runExit(fetchConnectionState()),
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
    detailQuery.data && Exit.isSuccess(detailQuery.data) ? detailQuery.data.value : null;
  const statusTitle = () => {
    const current = detailQuery.data;
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load item detail';
    }
    return 'Loading item detail';
  };
  const statusDescription = () => {
    const current = detailQuery.data;
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load item detail');
    }
    return 'JellyPilot is loading Movie or Episode detail data from Jellyfin.';
  };
  const openPlaybackChooser = (item: VideoItemDetail, mode: VideoLibraryPlayMode) => {
    if (confirmBusy()) {
      return;
    }

    setPlayError(null);
    setPendingPlayback({
      detail: item,
      mode,
      startPositionSeconds: mode === 'resume' ? item.resumePositionSeconds : 0,
    });
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

  return (
    <div class={styles.stack}>
      <Suspense fallback={<ItemDetailSkeleton />}>
        <Show
          when={detail()}
          fallback={<LibraryStatusPanel title={statusTitle()} description={statusDescription()} />}
        >
          {(item) => {
            const isEpisode = () => item().itemType === 'Episode';
            const resumeProgress = () =>
              item().canResume ? (item().playedPercentage ?? 0) / 100 : null;

            return (
              <>
                <DetailHero
                  title={item().name}
                  subtitle={detailSubtitleElement(item())}
                  backdropImageId={item().backdropImageId ?? null}
                  artworkImageId={item().artworkImageId ?? null}
                  artworkAspect={isEpisode() ? 'landscape' : 'poster'}
                  typeLabel={item().itemType}
                  typeIcon={
                    isEpisode() ? <Tv class={styles.icon6} /> : <Film class={styles.icon6} />
                  }
                  onBack={closeDetail}
                  badges={
                    <>
                      <StatusBadge variant={item().played ? 'success' : 'neutral'}>
                        {item().played ? 'Played' : 'Unplayed'}
                      </StatusBadge>
                      <StatusBadge variant={item().favorite ? 'success' : 'neutral'}>
                        {item().favorite ? 'Favorite' : 'Not favorite'}
                      </StatusBadge>
                      <Show when={formatRuntime(item().runtimeSeconds)}>
                        {(runtime) => <StatusBadge variant="neutral">{runtime()}</StatusBadge>}
                      </Show>
                    </>
                  }
                  actions={
                    <>
                      <Show
                        when={item().canResume}
                        fallback={
                          <Button
                            type="button"
                            variant="primary"
                            class={styles.pillButton}
                            disabled={!item().canPlay || confirmBusy()}
                            onClick={() => openPlaybackChooser(item(), 'start')}
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
                          disabled={!item().canPlay || confirmBusy()}
                          onClick={() => openPlaybackChooser(item(), 'resume')}
                          leadingIcon={<Play class={styles.playIcon} />}
                        >
                          Resume
                        </Button>
                        <Button
                          type="button"
                          variant="secondary"
                          class={styles.pillButton}
                          disabled={!item().canPlay || confirmBusy()}
                          onClick={() => openPlaybackChooser(item(), 'start')}
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
                  resumeProgress={resumeProgress()}
                />

                <div class={styles.content}>
                  <Show when={item().overview}>
                    {(overview) => <p class={styles.overview}>{overview()}</p>}
                  </Show>
                  <Show when={item().genres.length > 0}>
                    <div class={styles.pillRow}>
                      <For each={item().genres}>
                        {(genre) => <span class={styles.genre}>{genre}</span>}
                      </For>
                    </div>
                  </Show>
                </div>
              </>
            );
          }}
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

function ItemDetailSkeleton() {
  return (
    <article class={styles.stack} aria-hidden="true">
      <div class={styles.skeletonHero} />
      <div class={styles.skeletonContent}>
        <div class={styles.skeletonLine} />
        <div class={`${styles.skeletonLine} ${styles.skeletonLineShort}`} />
        <div class={styles.pillRow}>
          <For each={[0, 1, 2]}>{() => <div class={styles.skeletonPill} />}</For>
        </div>
      </div>
    </article>
  );
}
