import { Dialog } from '@ark-ui/solid/dialog';
import { useNavigate } from '@tanstack/solid-router';
import { Settings, X } from 'lucide-solid';

import OperationsConsole from './OperationsConsole';
import { Button } from './ui';

export default function SettingsModal() {
  const navigate = useNavigate();

  return (
    <Dialog.Root lazyMount unmountOnExit>
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

      <Dialog.Context>
        {(dialog) => (
          <>
            <Dialog.Backdrop class="fixed inset-0 z-40 bg-black/70 backdrop-blur-sm transition-all duration-300" />
            <Dialog.Positioner class="fixed inset-0 z-40 flex h-full w-full flex-col overflow-hidden">
              <Dialog.Content
                class="border-outline-variant/30 bg-surface-container-low/60 flex h-full w-full animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] flex-col overflow-hidden backdrop-blur-xl outline-none"
                onKeyDown={(event) => {
                  if (event.key === 'Escape') {
                    dialog().setOpen(false);
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
          </>
        )}
      </Dialog.Context>
    </Dialog.Root>
  );
}
