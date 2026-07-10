import { VideoHomeRow } from '@components/library/shared';
import { Card } from '@jellypilot/ui';
import { createQuery } from '@tanstack/solid-query';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { For, Show, createMemo } from 'solid-js';
import { fetchConnectionState } from '~effects/connection';
import { fetchLibraryHome } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';

import * as styles from '../library.css';

const homeSkeletonRows = [
  { id: 'continue-watching-skeleton', aspectClass: 'video' },
  { id: 'next-up-skeleton', aspectClass: 'video' },
  { id: 'latest-movies-skeleton', aspectClass: 'poster' },
  { id: 'latest-episodes-skeleton', aspectClass: 'video' },
] as const;

export const Route = createFileRoute('/_authenticated/library/')({
  component: LibraryLanding,
});

function LibraryLanding() {
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState()),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const homeQuery = createQuery(() => ({
    queryKey: queryKeys.libraryHome(sessionKey()),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchLibraryHome()),
  }));
  const home = () =>
    homeQuery.data && Exit.isSuccess(homeQuery.data) ? homeQuery.data.value : null;

  return (
    <div class={styles.stack}>
      <Show when={!homeQuery.isPending} fallback={<VideoHomeSkeleton />}>
        <Show when={home()}>
          {(value) => (
            <div class={styles.stack}>
              <VideoHomeRow
                id="continue-watching"
                title="Continue Watching"
                kind="continueWatching"
                items={value().continueWatching}
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
    </div>
  );
}

function VideoHomeSkeleton() {
  return (
    <div class={styles.stack} aria-hidden="true">
      <For each={homeSkeletonRows}>
        {(row) => (
          <section class={styles.skeletonRow}>
            <div class={styles.skeletonTitle} />
            <div class={styles.skeletonGrid}>
              <For each={[0, 1, 2, 3]}>
                {() => (
                  <Card variant="filled">
                    <div
                      class={`${styles.skeletonArtwork} ${styles.skeletonAspect[row.aspectClass]}`}
                    />
                    <div class={styles.skeletonBody}>
                      <div class={styles.skeletonLine.title} />
                      <div class={styles.skeletonLine.subtitle} />
                    </div>
                  </Card>
                )}
              </For>
            </div>
          </section>
        )}
      </For>
    </div>
  );
}
