import type { VideoHomeItem, VideoLibraryPlayRequest } from '@bindings';
import { useAuthenticatedBootstrap } from '@components/AuthenticatedBootstrap';
import { HomeFeaturedHero, HomeFeaturedHeroSkeleton } from '@components/library/HomeFeaturedHero';
import type { HomeFeaturedSource } from '@components/library/HomeFeaturedHero';
import { VideoHomeRow } from '@components/library/shared';
import { useToast } from '@components/ToastProvider';
import { cx } from '@styled-system/css';
import { createMutation, createQuery } from '@tanstack/solid-query';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { For, Show, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchLibraryHome, fetchVideoItemDetail, startLibraryPlayback } from '~effects/library';
import { queryKeys, runExit } from '~effects/query';
import { videoHomePlaybackDecision } from '~utils/videoHomeLayout';

import * as styles from '../library.styles';

const homeSkeletonRows = [
  { id: 'continue-watching-skeleton', aspectClass: 'video', cardCount: 4 },
  { id: 'latest-movies-skeleton', aspectClass: 'poster', cardCount: 6 },
  { id: 'next-up-skeleton', aspectClass: 'video', cardCount: 4 },
  { id: 'latest-episodes-skeleton', aspectClass: 'video', cardCount: 4 },
] as const;

export const Route = createFileRoute('/_authenticated/library/')({
  component: LibraryLanding,
});

interface HomeFeature {
  source: HomeFeaturedSource;
  item: VideoHomeItem;
}

function LibraryLanding() {
  const { showToast } = useToast();
  const bootstrap = useAuthenticatedBootstrap();
  const sessionKey = bootstrap.sessionKey;
  const homeQuery = createQuery(() => ({
    queryKey: queryKeys.libraryHome(sessionKey()),
    enabled: bootstrap.connected(),
    queryFn: () => runExit(fetchLibraryHome),
  }));
  const home = () =>
    homeQuery.data && Exit.isSuccess(homeQuery.data) ? homeQuery.data.value : null;
  const playbackMutation = createMutation(() => ({
    mutationFn: (request: VideoLibraryPlayRequest) => runExit(startLibraryPlayback(request)),
  }));
  const [playbackBusyId, setPlaybackBusyId] = createSignal<string | null>(null);

  // Resume-first feature precedence: first resumable Continue Watching item,
  // otherwise first Next Up, otherwise first Latest Movie. Row membership is
  // authoritative; the featured item stays in its row.
  const feature = createMemo((): HomeFeature | null => {
    const value = home();
    if (!value) {
      return null;
    }
    const resumable = value.continueWatching.find(
      (item) => videoHomePlaybackDecision(item).mode === 'resume',
    );
    if (resumable) {
      return { source: 'continueWatching', item: resumable };
    }
    const nextUp = value.nextUp[0];
    if (nextUp) {
      return { source: 'nextUp', item: nextUp };
    }
    const latestMovie = value.latestMovies[0];
    if (latestMovie) {
      return { source: 'latestMovies', item: latestMovie };
    }
    return null;
  });

  const featureDetailQuery = createQuery(() => ({
    queryKey: queryKeys.libraryItemDetail(sessionKey(), feature()?.item.id ?? ''),
    enabled: bootstrap.connected() && feature() !== null,
    queryFn: () => runExit(fetchVideoItemDetail(feature()?.item.id ?? '')),
  }));
  const featureDetail = () => {
    const data = featureDetailQuery.data;
    return data && Exit.isSuccess(data) ? data.value : null;
  };

  const playHomeItem = async (item: VideoHomeItem) => {
    if (playbackBusyId() !== null) {
      return;
    }
    const decision = videoHomePlaybackDecision(item);

    setPlaybackBusyId(item.id);
    const result = await playbackMutation.mutateAsync({
      audioStreamIndex: null,
      itemId: item.id,
      mode: decision.mode,
      startPositionSeconds: decision.startPositionSeconds,
      subtitleStreamIndex: null,
    });
    const message = Exit.match(result, {
      onFailure: (cause) =>
        commandFailureMessage(
          cause,
          decision.mode === 'resume' ? 'Could not resume playback' : 'Could not start playback',
        ),
      onSuccess: () => null,
    });
    setPlaybackBusyId(null);
    if (message) {
      showToast('error', message);
    }
  };

  return (
    <Show when={bootstrap.connected()}>
      <Show when={!homeQuery.isPending} fallback={<VideoHomeSkeleton />}>
        <Show when={home()}>
          {(value) => (
            <div class={styles.homeContent}>
              <Show when={feature()}>
                {(current) => (
                  <Show
                    when={!featureDetailQuery.isPending}
                    fallback={<HomeFeaturedHeroSkeleton />}
                  >
                    <HomeFeaturedHero
                      source={current().source}
                      item={current().item}
                      detail={featureDetail()}
                      busy={playbackBusyId() === current().item.id}
                      playbackDisabled={playbackBusyId() !== null}
                      onPlay={() => void playHomeItem(current().item)}
                    />
                  </Show>
                )}
              </Show>
              <VideoHomeRow
                id="continue-watching"
                title="Continue Watching"
                kind="continueWatching"
                items={value().continueWatching}
                playbackBusyId={playbackBusyId()}
                onPlay={(item) => void playHomeItem(item)}
              />
              <VideoHomeRow
                id="latest-movies"
                title="Latest Movies"
                kind="latestMovies"
                items={value().latestMovies}
              />
              <VideoHomeRow
                id="next-up"
                title="Next Up"
                kind="nextUp"
                items={value().nextUp}
                playbackBusyId={playbackBusyId()}
                onPlay={(item) => void playHomeItem(item)}
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
    </Show>
  );
}

function VideoHomeSkeleton() {
  return (
    <div class={styles.homeContent} aria-hidden="true">
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
