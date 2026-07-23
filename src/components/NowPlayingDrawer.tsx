import { Dialog } from '@ark-ui/solid/dialog';
import { Match } from 'effect';
import { MonitorPlay, X } from 'lucide-solid';
import { Show, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';
import * as recipes from '~styles/recipes';

import type { NowPlayingState } from '../bindings';
import { createNowPlayingState } from '../effects/nowPlaying';
import NowPlayingCard from './NowPlayingCard';
import * as styles from './NowPlayingDrawer.styles';
import { Button } from './ui';

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

export default function NowPlayingDrawer(props: {
  jellyfinConnected: boolean;
  collapsed: boolean;
}) {
  const { state } = createNowPlayingState();
  const [open, setOpen] = createSignal(false);
  const [selectPortalMount, setSelectPortalMount] = createSignal<HTMLElement>();

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
            size="row"
            aria-label={triggerLabel(state())}
            class={styles.trigger({ collapsed: props.collapsed })}
          >
            <span class={styles.triggerIconWrap}>
              <MonitorPlay class={styles.triggerIcon} />
              <span class={statusDotClass(state()?.status)} />
            </span>
            <span class={styles.triggerLabel({ collapsed: props.collapsed })}>Now Playing</span>
          </Button>
        )}
      />

      <Portal>
        <Dialog.Backdrop class={recipes.scrim({ tone: 'surface', z: '50' })} />
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
