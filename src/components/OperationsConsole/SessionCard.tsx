import { Dialog } from '@ark-ui/solid/dialog';
import { LogOut, ShieldAlert } from 'lucide-solid';
import { Portal } from 'solid-js/web';

import { Button, Card } from '../ui';
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
      <Card
        as="section"
        variant="filled"
        class="border-error/20 bg-error-container/5 hover:border-error/45"
      >
        <div class="flex items-start gap-3">
          <ShieldAlert class="text-error mt-1 h-5 w-5 drop-shadow-[0_0_8px_rgba(255,107,122,0.4)]" />
          <div>
            <h2 class="text-on-surface text-[16px] leading-[24px] font-semibold">Session</h2>
            <p class="text-on-surface-variant/80 mt-1 text-[12px] leading-[16px]">
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
              class="border-error/55 text-error hover:bg-error/10 hover:border-error mt-5 w-full"
            >
              <LogOut class="h-4.5 w-4.5" />
              <span>Sign out</span>
            </Button>
          )}
        />
      </Card>

      <Portal>
        <Dialog.Backdrop
          class="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm transition-[backdrop-filter,background-color,opacity] duration-300 data-[state=closed]:opacity-0 data-[state=open]:opacity-100"
          onClick={() => {
            if (!ui.signingOut) {
              actions.setSignOutDialogOpen(false);
            }
          }}
        />
        <Dialog.Positioner class="fixed inset-0 z-50 flex items-center justify-center p-4">
          <Dialog.Content
            class="border-primary/20 bg-surface-container-low/45 hover:border-primary/35 hover:bg-surface-container-low/60 border-error/30 relative max-w-md overflow-hidden rounded-[2rem] border p-6 shadow-2xl backdrop-blur-xl transition-[background-color,border-color,box-shadow,opacity,transform] duration-300 data-[state=closed]:translate-y-1 data-[state=closed]:opacity-0 data-[state=open]:translate-y-0 data-[state=open]:opacity-100"
            onKeyDown={(event) => {
              if (event.key === 'Escape' && !ui.signingOut) {
                actions.setSignOutDialogOpen(false);
              }
            }}
          >
            {/* Red top glow bar */}
            <div class="via-error/60 absolute top-0 left-0 h-[3px] w-full bg-gradient-to-r from-transparent to-transparent" />

            <Dialog.Title
              id="sign-out-title"
              class="text-on-surface flex items-center gap-2 text-[22px] leading-[28px] font-bold"
            >
              <ShieldAlert class="text-error h-6 w-6" />
              Sign out?
            </Dialog.Title>
            <Dialog.Description class="text-on-surface-variant/90 mt-3 text-[14px] leading-[20px]">
              This removes only the active saved service profile. Other saved services remain
              available in Settings.
            </Dialog.Description>
            <div class="mt-6 flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
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
                class="border-error/60 text-error hover:bg-error/10"
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
