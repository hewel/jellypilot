import { Dialog } from '@ark-ui/solid/dialog';
import { MonitorPlay, X } from 'lucide-solid';
import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';

import { commands, events } from '../bindings';
import type { NowPlayingState } from '../bindings';
import NowPlayingCard from './NowPlayingCard';
import { Button } from './ui';

function statusText(status?: NowPlayingState['status']): string {
  switch (status) {
    case 'playing': {
      return 'Playing';
    }
    case 'paused': {
      return 'Paused';
    }
    case 'idle': {
      return 'MPV idle';
    }
    case 'offline': {
      return 'Player offline';
    }
    default: {
      return 'Playback unknown';
    }
  }
}

function statusDotClass(status?: NowPlayingState['status']): string {
  switch (status) {
    case 'playing':
    case 'paused': {
      return 'bg-tertiary shadow-[0_0_8px_var(--color-tertiary)]';
    }
    case 'offline': {
      return 'bg-error shadow-[0_0_8px_var(--color-error)]';
    }
    default: {
      return 'bg-outline-variant';
    }
  }
}

function triggerLabel(state: NowPlayingState | null): string {
  const status = statusText(state?.status);
  const media = state?.media;
  if (media?.name) {
    return `Now Playing: ${status} — ${media.name}`;
  }
  return `Now Playing: ${status}`;
}

export default function NowPlayingDrawer(props: { jellyfinConnected: boolean }) {
  const [state, setState] = createSignal<NowPlayingState | null>(null);
  const [open, setOpen] = createSignal(false);
  const [selectPortalMount, setSelectPortalMount] = createSignal<HTMLElement>();

  onMount(() => {
    void commands.nowPlayingGetState().then((result) => {
      if (result.status === 'ok') {
        setState(result.data);
      }
    });

    let disposed = false;
    let cleanup: (() => void) | undefined;
    events.nowPlayingChanged
      .listen((event) => setState(event.payload.state))
      .then((unlisten) => {
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
            class="relative"
          >
            <MonitorPlay class="h-5 w-5" />
            <span
              class={`absolute top-1 right-1 h-2 w-2 rounded-full ${statusDotClass(state()?.status)}`}
            />
          </Button>
        )}
      />

      <Portal>
        <Dialog.Backdrop class="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm transition-all duration-300" />
        <Dialog.Positioner class="fixed inset-0 z-50 flex justify-end">
          <Dialog.Content
            ref={setSelectPortalMount}
            class="border-outline-variant/30 bg-surface-container-low/60 flex h-full w-full animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] flex-col overflow-hidden rounded-l-[2rem] border-l shadow-2xl backdrop-blur-xl sm:w-[28rem]"
          >
            {/* Header */}
            <div class="border-outline-variant/20 flex items-center justify-between border-b px-5 py-4">
              <div>
                <Dialog.Title class="text-on-surface text-[18px] leading-[24px] font-bold">
                  Now Playing
                </Dialog.Title>
                <Dialog.Description class="text-on-surface-variant/70 mt-0.5 text-[12px] leading-[16px]">
                  Playback details and MPV controls
                </Dialog.Description>
              </div>
              <Button
                type="button"
                variant="icon"
                aria-label="Close Now Playing"
                onClick={() => setOpen(false)}
              >
                <X class="h-5 w-5" />
              </Button>
            </div>

            {/* Body */}
            <div class="flex-1 overflow-y-auto px-5 py-4">
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
