import type { VideoHomeItem, VideoLibraryPlayRequest } from '@bindings';
import { VideoHomeRow } from '@components/library/shared';
import { useToast } from '@components/ToastProvider';
import { cx } from '@styled-system/css';
import { createMutation, createQuery } from '@tanstack/solid-query';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { For, Show, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import { fetchLibraryHome, startLibraryPlayback } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';
import { isValidVideoHomeResumePosition } from '~utils/videoHomeLayout';

import * as styles from '../library.styles';

const homeSkeletonRows = [
  { id: 'continue-watching-skeleton', aspectClass: 'video', cardCount: 3 },
  { id: 'next-up-skeleton', aspectClass: 'video', cardCount: 3 },
  { id: 'latest-movies-skeleton', aspectClass: 'poster', cardCount: 5 },
  { id: 'latest-episodes-skeleton', aspectClass: 'video', cardCount: 3 },
] as const;

export const Route = createFileRoute('/_authenticated/library/')({
  component: LibraryLanding,
});

function LibraryLanding() {
  const { showToast } = useToast();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const homeQuery = createQuery(() => ({
    queryKey: queryKeys.libraryHome(sessionKey()),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchLibraryHome),
  }));
  const home = () =>
    homeQuery.data && Exit.isSuccess(homeQuery.data) ? homeQuery.data.value : null;
  const playbackMutation = createMutation(() => ({
    mutationFn: (request: VideoLibraryPlayRequest) => runExit(startLibraryPlayback(request)),
  }));
  const [resumeBusyId, setResumeBusyId] = createSignal<string | null>(null);

  const resumeItem = async (item: VideoHomeItem) => {
    if (resumeBusyId() !== null) {
      return;
    }
    const startPositionSeconds = item.resumePositionSeconds;
    if (!isValidVideoHomeResumePosition(startPositionSeconds, item.runtimeSeconds)) {
      return;
    }

    setResumeBusyId(item.id);
    const result = await playbackMutation.mutateAsync({
      audioStreamIndex: null,
      itemId: item.id,
      mode: 'resume',
      startPositionSeconds,
      subtitleStreamIndex: null,
    });
    const message = Exit.match(result, {
      onFailure: (cause) => commandFailureMessage(cause, 'Could not resume playback'),
      onSuccess: () => null,
    });
    setResumeBusyId(null);
    if (message) {
      showToast('error', message);
    }
  };

  return (
    <Show when={!homeQuery.isPending} fallback={<VideoHomeSkeleton />}>
      <Show when={home()}>
        {(value) => (
          <div class={styles.stack}>
            <VideoHomeRow
              id="continue-watching"
              title="Continue Watching"
              kind="continueWatching"
              items={value().continueWatching}
              resumeBusyId={resumeBusyId()}
              onResume={(item) => void resumeItem(item)}
            />
            <VideoHomeRow id="next-up" title="Next Up" kind="nextUp" items={value().nextUp} />
            <VideoHomeRow
              id="latest-movies"
              title="Latest Movies"
              kind="latestMovies"
              items={value().latestMovies}
            />
            <VideoHomeRow
              id="latest-episodes"
              title="Latest Episodes"
              kind="latestEpisodes"
              items={value().latestEpisodes}
            />
          </div>
        )}
      </Show>
    </Show>
  );
}

function VideoHomeSkeleton() {
  return (
    <div class={styles.stack} aria-hidden="true">
      <For each={homeSkeletonRows}>
        {(row) => (
          <section class={styles.skeletonRow}>
            <div class={styles.skeletonHeader}>
              <div class={styles.skeletonTitle} />
              <div class={styles.skeletonAction} />
            </div>
            <div class={styles.skeletonGrid[row.aspectClass]}>
              <For each={Array.from({ length: row.cardCount }, (_, index) => index)}>
                {() => (
                  <div class={styles.skeletonCard}>
                    <div
                      class={cx(styles.skeletonArtwork, styles.skeletonAspect[row.aspectClass])}
                    />
                    <div class={styles.skeletonBody}>
                      <div class={styles.skeletonLine.title} />
                      <div class={styles.skeletonLine.subtitle} />
                    </div>
                  </div>
                )}
              </For>
            </div>
          </section>
        )}
      </For>
    </div>
  );
}
