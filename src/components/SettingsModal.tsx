import { useNavigate } from '@tanstack/solid-router';
import { Settings, X } from 'lucide-solid';
import { Show, createEffect, createSignal, onCleanup } from 'solid-js';

import OperationsConsole from './OperationsConsole';
import { Button } from './ui';

const SETTINGS_TITLE_ID = 'settings-dialog-title';
const SETTINGS_DESC_ID = 'settings-dialog-description';

export default function SettingsModal() {
  const [open, setOpen] = createSignal(false);
  const navigate = useNavigate();
  let panelRef: HTMLElement | undefined;

  createEffect(() => {
    if (!open()) {
      return;
    }
    const node = panelRef;
    if (!node) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false);
      }
    };
    node.addEventListener('keydown', onKeyDown);
    queueMicrotask(() => node.focus());
    onCleanup(() => node.removeEventListener('keydown', onKeyDown));
  });

  return (
    <>
      <Button
        type="button"
        variant="primary"
        size="lg"
        aria-label="Open Settings"
        aria-expanded={open()}
        onClick={() => setOpen(true)}
        class="shadow-primary/45 shadow-2xl"
      >
        <Settings class="h-5 w-5" />
        <span>Settings</span>
      </Button>
      <Show when={open()}>
        <div
          ref={(el) => {
            panelRef = el;
          }}
          role="dialog"
          aria-modal="true"
          aria-labelledby={SETTINGS_TITLE_ID}
          aria-describedby={SETTINGS_DESC_ID}
          tabindex={-1}
          class="border-outline-variant/30 bg-surface-container-low/60 fixed inset-0 z-40 flex h-full w-full animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] flex-col overflow-hidden backdrop-blur-xl outline-none"
        >
          <header class="border-outline-variant/40 bg-surface-container-low/70 flex items-center justify-between gap-3 border-b px-5 py-4 backdrop-blur-xl">
            <div>
              <h2
                id={SETTINGS_TITLE_ID}
                class="text-on-surface text-[22px] leading-[28px] font-bold"
              >
                Settings
              </h2>
              <p
                id={SETTINGS_DESC_ID}
                class="text-on-surface-variant/70 mt-0.5 text-[12px] leading-[16px]"
              >
                Connection, player bridge, diagnostics, shortcuts, and session controls
              </p>
            </div>
            <Button
              type="button"
              variant="icon"
              aria-label="Close Settings"
              onClick={() => setOpen(false)}
            >
              <X class="h-5 w-5" />
            </Button>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <OperationsConsole onSignedOut={() => navigate({ to: '/login' })} />
          </div>
        </div>
      </Show>
    </>
  );
}
