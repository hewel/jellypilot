import { Dialog } from '@ark-ui/solid/dialog';
import { createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Match } from 'effect';
import { MonitorPlay, X } from 'lucide-solid';
import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';

import type { NowPlayingState } from '../bindings';
import { fetchNowPlayingState, listenNowPlayingChanged } from '../effects/nowPlaying';
import { queryKeys, runExit } from '../effects/query';
import NowPlayingCard from './NowPlayingCard';
import * as rootStyles from './NowPlayingDrawer.styles';
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
    <Dialog.Root
      open={open()}
      onOpenChange={(details) => setOpen(details.open)}
      closeOnEscape
      closeOnInteractOutside
      lazyMount
      unmountOnExit
      role="dialog"
    >
      <Dialog.Trigger
        asChild={(triggerProps) => (
          <Button
            {...triggerProps()}
            type="button"
            variant="icon"
            aria-label={triggerLabel(state())}
            class={rootStyles.trigger}
          >
            <MonitorPlay class={styles.triggerIcon} />
            <span class={statusDotClass(state()?.status)} />
          </Button>
        )}
      />

      <Portal>
        <Dialog.Backdrop class={styles.backdrop} />
        <Dialog.Positioner class={styles.positioner}>
          <Dialog.Content ref={setSelectPortalMount} class={styles.content}>
            <div class={styles.header}>
              <div>
                <Dialog.Title class={styles.title}>Now Playing</Dialog.Title>
                <Dialog.Description class={styles.description}>
                  Playback details and MPV controls
                </Dialog.Description>
              </div>
              <Button
                type="button"
                variant="icon"
                aria-label="Close Now Playing"
                onClick={() => setOpen(false)}
              >
                <X class={styles.closeIcon} />
              </Button>
            </div>

            <div class={styles.body}>
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
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
