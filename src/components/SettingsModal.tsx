import { Dialog } from '@ark-ui/solid/dialog';
import { useNavigate } from '@tanstack/solid-router';
import { Settings, X } from 'lucide-solid';
import { createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';

import OperationsConsole from './OperationsConsole';
import { Button } from './ui';

import * as styles from './SettingsModal.css';

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
            class={styles.trigger}
          >
            <Settings class={styles.triggerIcon} />
          </Button>
        )}
      />
      <Portal>
        <Dialog.Backdrop class={styles.backdrop} />
        <Dialog.Positioner class={styles.positioner}>
          <Dialog.Content
            class={styles.content}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                setOpen(false);
              }
            }}
          >
            <header class={styles.header}>
              <div>
                <Dialog.Title class={styles.title}>Settings</Dialog.Title>
                <Dialog.Description class={styles.description}>
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
                    <X class={styles.closeIcon} />
                  </Button>
                )}
              />
            </header>
            <div class={styles.body}>
              <OperationsConsole onSignedOut={() => navigate({ to: '/login' })} />
            </div>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
