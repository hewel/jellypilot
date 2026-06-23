import { Dialog } from '@ark-ui/solid/dialog';
import { useNavigate } from '@tanstack/solid-router';
import { Settings, X } from 'lucide-solid';
import { createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';

import OperationsConsole from './OperationsConsole';
import { Button } from './ui';

export default function SettingsModal() {
  const navigate = useNavigate();
  const [open, setOpen] = createSignal(false);

  return (
    <Dialog.Root open={open()} onOpenChange={(e) => setOpen(e.open)} lazyMount unmountOnExit>
      <Dialog.Trigger
        asChild={(triggerProps) => (
          <Button
            {...triggerProps()}
            type="button"
            variant="primary"
            size="lg"
            aria-label="Open Settings"
            class="shadow-secondary/45 h-13 w-13 p-0! shadow-2xl"
          >
            <Settings class="h-5 w-5" />
          </Button>
        )}
      />
      <Portal>
        <Dialog.Backdrop class="fixed inset-0 z-40 bg-black/70 backdrop-blur-sm transition-[backdrop-filter,background-color,opacity] duration-300 data-[state=closed]:opacity-0 data-[state=open]:opacity-100" />
        <Dialog.Positioner class="fixed inset-0 z-40 flex h-full w-full flex-col overflow-hidden">
          <Dialog.Content
            class="border-outline-variant/30 bg-surface-container-low/60 flex h-full w-full flex-col overflow-hidden backdrop-blur-xl transition-[opacity,transform] duration-200 ease-[cubic-bezier(0.2,0,0,1)] outline-none data-[state=closed]:translate-y-1 data-[state=closed]:opacity-0 data-[state=open]:translate-y-0 data-[state=open]:opacity-100"
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                setOpen(false);
              }
            }}
          >
            <header class="border-outline-variant/40 bg-surface-container-low/70 flex items-center justify-between gap-3 border-b px-5 py-4 backdrop-blur-xl">
              <div>
                <Dialog.Title class="text-on-surface text-[22px] leading-7 font-bold">
                  Settings
                </Dialog.Title>
                <Dialog.Description class="text-on-surface-variant/70 mt-0.5 text-[12px] leading-4">
                  Connection, player bridge, diagnostics, shortcuts, and session controls
                </Dialog.Description>
              </div>
              <Dialog.CloseTrigger
                asChild={(closeProps) => (
                  <Button
                    {...closeProps()}
                    type="button"
                    variant="icon"
                    aria-label="Close Settings"
                  >
                    <X class="h-5 w-5" />
                  </Button>
                )}
              />
            </header>
            <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
              <OperationsConsole onSignedOut={() => navigate({ to: '/login' })} />
            </div>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
