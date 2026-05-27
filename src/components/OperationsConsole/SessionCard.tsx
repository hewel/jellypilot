import { Dialog } from '@ark-ui/solid/dialog';
import { LogOut, ShieldAlert } from 'lucide-solid';
import { Portal } from 'solid-js/web';
import { useOperationsConsoleStore } from './store';

interface SessionCardProps {
  onSignOut: () => void;
}

export default function SessionCard(props: SessionCardProps) {
  const [ui, actions] = useOperationsConsoleStore();

  return (
    <Dialog.Root
      open={ui.confirmSignOut}
      onOpenChange={(details) => {
        if (ui.signingOut && !details.open) return;
        actions.setSignOutDialogOpen(details.open);
      }}
      closeOnEscape={!ui.signingOut}
      closeOnInteractOutside={!ui.signingOut}
      onEscapeKeyDown={() => {
        if (!ui.signingOut) actions.setSignOutDialogOpen(false);
      }}
      onInteractOutside={() => {
        if (!ui.signingOut) actions.setSignOutDialogOpen(false);
      }}
      lazyMount
      unmountOnExit
      role="dialog"
    >
      <section class="card-filled border-error/30">
        <div class="flex items-start gap-3">
          <ShieldAlert class="mt-1 h-5 w-5 text-error" />
          <div>
            <h2 class="text-title-medium text-on-surface">Session</h2>
            <p class="mt-1 text-body-small text-on-surface-variant">
              Sign out removes the Saved Session and requires authentication
              before Reconnect is available.
            </p>
          </div>
        </div>
        <Dialog.Trigger class="btn-outlined mt-5 w-full border-error/60 text-error hover:bg-error/10">
          <LogOut class="h-5 w-5" />
          Sign out
        </Dialog.Trigger>
      </section>

      <Portal>
        <Dialog.Backdrop
          class="fixed inset-0 z-50 bg-black/60"
          onClick={() => {
            if (!ui.signingOut) actions.setSignOutDialogOpen(false);
          }}
        />
        <Dialog.Positioner class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <Dialog.Content
            class="card-elevated max-w-md"
            onKeyDown={(event) => {
              if (event.key === 'Escape' && !ui.signingOut) {
                actions.setSignOutDialogOpen(false);
              }
            }}
          >
            <Dialog.Title
              id="sign-out-title"
              class="text-title-large text-on-surface"
            >
              Sign out?
            </Dialog.Title>
            <Dialog.Description class="mt-3 text-body-medium text-on-surface-variant">
              This removes the Saved Session and you'll need to authenticate
              again before reconnecting.
            </Dialog.Description>
            <div class="mt-6 flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
              <button
                type="button"
                class="btn-secondary"
                onClick={() => actions.setSignOutDialogOpen(false)}
                disabled={ui.signingOut}
              >
                Cancel
              </button>
              <button
                type="button"
                class="btn-outlined border-error/60 text-error hover:bg-error/10"
                onClick={props.onSignOut}
                disabled={ui.signingOut}
              >
                {ui.signingOut ? 'Signing out...' : 'Sign out'}
              </button>
            </div>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
