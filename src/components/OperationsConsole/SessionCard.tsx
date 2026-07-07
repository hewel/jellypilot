import { LogOut, ShieldAlert } from 'lucide-solid';
import { Portal } from 'solid-js/web';

import { Button, Card, Dialog } from '../ui';
import { useOperationsConsoleStore } from './store';

import * as patterns from '../../styles/patterns.css';
import * as styles from './SessionCard.css';

interface SessionCardProps {
  onSignOut: () => void;
}

export default function SessionCard(props: SessionCardProps) {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <Dialog.Root
      open={ui.confirmSignOut}
      onOpenChange={(details) => {
        if (ui.signingOut && !details.open) {
          return;
        }
        actions.setSignOutDialogOpen(details.open);
      }}
      closeOnEscape={!ui.signingOut}
      closeOnInteractOutside={!ui.signingOut}
      onEscapeKeyDown={() => {
        if (!ui.signingOut) {
          actions.setSignOutDialogOpen(false);
        }
      }}
      onInteractOutside={() => {
        if (!ui.signingOut) {
          actions.setSignOutDialogOpen(false);
        }
      }}
      lazyMount
      unmountOnExit
      role="dialog"
    >
      <Card as="section" variant="filled" class={styles.card}>
        <div class={styles.header}>
          <ShieldAlert class={styles.cardIcon} />
          <div>
            <h2 class={styles.title}>Session</h2>
            <p class={styles.description}>
              Sign out removes the active saved service and leaves any other saved services
              available.
            </p>
          </div>
        </div>
        <Dialog.Trigger
          asChild={(triggerProps) => (
            <Button
              {...triggerProps()}
              type="button"
              variant="outlined"
              class={styles.signOutButton}
            >
              <LogOut class={patterns.icon4_5} />
              <span>Sign out</span>
            </Button>
          )}
        />
      </Card>

      <Portal>
        <Dialog.Backdrop
          class={styles.backdrop}
          onClick={() => {
            if (!ui.signingOut) {
              actions.setSignOutDialogOpen(false);
            }
          }}
        />
        <Dialog.Positioner class={`${styles.positioner} ${styles.positionerFill}`}>
          <Dialog.Content class={styles.content}>
            {/* Red top glow bar */}
            <div class={styles.glow} />

            <Dialog.Title id="sign-out-title" class={styles.dialogTitle}>
              <ShieldAlert class={styles.dialogIcon} />
              Sign out?
            </Dialog.Title>
            <Dialog.Description class={styles.dialogDescription}>
              This removes only the active saved service profile. Other saved services remain
              available in Settings.
            </Dialog.Description>
            <div class={styles.actions}>
              <Button
                type="button"
                variant="secondary"
                onClick={() => actions.setSignOutDialogOpen(false)}
                disabled={ui.signingOut}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="outlined"
                class={styles.dangerButton}
                onClick={props.onSignOut}
                disabled={ui.signingOut}
              >
                {ui.signingOut ? 'Signing out...' : 'Sign out'}
              </Button>
            </div>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
