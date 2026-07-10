import { Dialog } from '@jellypilot/ui';
import { createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Match } from 'effect';
import { MonitorPlay, X } from 'lucide-solid';
import { Show, createSignal, onCleanup, onMount } from 'solid-js';

import type { NowPlayingState } from '../bindings';
import { fetchNowPlayingState, listenNowPlayingChanged } from '../effects/nowPlaying';
import { queryKeys, runExit } from '../effects/query';
import NowPlayingCard from './NowPlayingCard';
import { Button } from './ui';

import * as styles from './NowPlayingDrawer.css';

const statusText = Match.type<NowPlayingState['status'] | undefined>().pipe(
  Match.withReturnType<string>(),
  Match.when('playing', () => 'Playing'),
  Match.when('paused', () => 'Paused'),
  Match.when('idle', () => 'MPV idle'),
  Match.when('offline', () => 'Player offline'),
  Match.orElse(() => 'Playback unknown'),
);

const statusDotClass = Match.type<NowPlayingState['status'] | undefined>().pipe(
  Match.withReturnType<string>(),
  Match.when('playing', () => styles.statusDot({ status: 'playing' })),
  Match.when('paused', () => styles.statusDot({ status: 'paused' })),
  Match.when('idle', () => styles.statusDot({ status: 'idle' })),
  Match.when('offline', () => styles.statusDot({ status: 'offline' })),
  Match.orElse(() => styles.statusDot({ status: 'unknown' })),
);

function triggerLabel(state: NowPlayingState | null): string {
  const status = statusText(state?.status);
  const media = state?.media;
  if (media?.name) {
    return `Now Playing: ${status} — ${media.name}`;
  }
  return `Now Playing: ${status}`;
}

export default function NowPlayingDrawer(props: { jellyfinConnected: boolean }) {
  const queryClient = useQueryClient();
  const nowPlayingQuery = createQuery(() => ({
    queryKey: queryKeys.nowPlayingState,
    queryFn: () => runExit(fetchNowPlayingState()),
  }));
  const state = () =>
    nowPlayingQuery.data && Exit.isSuccess(nowPlayingQuery.data)
      ? nowPlayingQuery.data.value
      : null;
  const [open, setOpen] = createSignal(false);
  const [selectPortalMount, setSelectPortalMount] = createSignal<HTMLElement>();

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    listenNowPlayingChanged((newState) =>
      queryClient.setQueryData(queryKeys.nowPlayingState, Exit.succeed(newState)),
    ).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        cleanup = unlisten;
      }
    });

    onCleanup(() => {
      disposed = true;
      cleanup?.();
    });
  });

  return (
    <>
      <Button
        type="button"
        variant="icon"
        aria-label={triggerLabel(state())}
        class={styles.trigger}
        onClick={() => setOpen(true)}
      >
        <MonitorPlay class={styles.triggerIcon} />
        <span class={statusDotClass(state()?.status)} />
      </Button>
      <Dialog
        open={open()}
        title="Now Playing"
        description="Playback details and MPV controls"
        onOpenChange={(next: boolean) => setOpen(next)}
        class={styles.content}
      >
        <div ref={setSelectPortalMount} class={styles.body}>
          <Show when={selectPortalMount()}>
            {(mount) => (
              <NowPlayingCard
                jellyfinConnected={props.jellyfinConnected}
                bare
                trackSelectPortalMount={mount()}
              />
            )}
          </Show>
        </div>
        <button
          type="button"
          class={styles.srOnlyClose}
          aria-label="Close Now Playing"
          onClick={() => setOpen(false)}
        >
          <X />
        </button>
      </Dialog>
    </>
  );
}
