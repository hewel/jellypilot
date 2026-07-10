import { AlertDialog, Button, Card, Heading } from '@jellypilot/ui';
import { LogOut, ShieldAlert } from 'lucide-solid';

import { useOperationsConsoleStore } from './store';

import * as patterns from '../../styles/patterns.css';
import * as styles from './SessionCard.css';

interface SessionCardProps {
  onSignOut: () => void;
}

export default function SessionCard(props: SessionCardProps) {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <>
      <Card class={styles.card}>
        <div class={styles.header}>
          <ShieldAlert class={styles.cardIcon} />
          <div>
            <Heading level={2} class={styles.title}>
              Session
            </Heading>
            <p class={styles.description}>
              Sign out removes the active saved service and leaves any other saved services
              available.
            </p>
          </div>
        </div>
        <Button
          type="button"
          variant="outline"
          class={styles.signOutButton}
          onClick={() => actions.setSignOutDialogOpen(true)}
        >
          <LogOut class={patterns.icon4_5} />
          <span>Sign out</span>
        </Button>
      </Card>

      <AlertDialog
        open={ui.confirmSignOut}
        title={
          <>
            <ShieldAlert class={styles.dialogIcon} />
            Sign out?
          </>
        }
        description="This removes only the active saved service profile. Other saved services remain available in Settings."
        cancelLabel="Cancel"
        actionLabel={ui.signingOut ? 'Signing out...' : 'Sign out'}
        dismissable={!ui.signingOut}
        closeOnAction={false}
        actionDisabled={ui.signingOut}
        cancelDisabled={ui.signingOut}
        onOpenChange={(next) => {
          if (!ui.signingOut) actions.setSignOutDialogOpen(next);
        }}
        onAction={() => {
          if (!ui.signingOut) props.onSignOut();
        }}
      />
    </>
  );
}
