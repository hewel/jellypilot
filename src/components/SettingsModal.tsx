import type { ThemePreference } from '@bindings';
import { Dialog } from '@jellypilot/ui';
import { useNavigate } from '@tanstack/solid-router';
import { Settings, X } from 'lucide-solid';
import { createSignal } from 'solid-js';

import { useConfigCoordinator } from '../effects/configContext';
import OperationsConsole from './OperationsConsole';
import { useToast } from './ToastProvider';
import { Button } from './ui';

import * as styles from './SettingsModal.css';

const THEME_OPTIONS: { value: ThemePreference; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

export default function SettingsModal() {
  const navigate = useNavigate();
  const [open, setOpen] = createSignal(false);
  const { coordinator, state } = useConfigCoordinator();
  const toast = useToast();
  const preference = () =>
    state().desired?.themePreference ?? state().confirmed?.themePreference ?? 'system';

  return (
    <>
      <Button
        type="button"
        variant="primary"
        size="lg"
        aria-label="Open Settings"
        class={styles.trigger}
        onClick={() => setOpen(true)}
      >
        <Settings class={styles.triggerIcon} />
      </Button>
      <Dialog
        open={open()}
        title="Settings"
        description="Connection, player bridge, diagnostics, shortcuts, and session controls"
        onOpenChange={(next: boolean) => setOpen(next)}
        class={styles.content}
      >
        <div class={styles.appearanceSection}>
          <h3 class={styles.appearanceTitle}>Appearance</h3>
          <p class={styles.appearanceDescription}>
            Choose System, Light, or Dark. Changes save through the shared configuration
            coordinator.
          </p>
          <div role="radiogroup" aria-label="Appearance theme" class={styles.appearanceOptions}>
            {THEME_OPTIONS.map((option) => (
              <button
                type="button"
                role="radio"
                aria-checked={preference() === option.value}
                data-selected={preference() === option.value ? 'true' : 'false'}
                class={styles.appearanceOption}
                onClick={() => {
                  const previous = preference();
                  coordinator.setThemePreference(option.value);
                  const stop = coordinator.subscribe((snapshot) => {
                    if (snapshot.status === 'error') {
                      toast.showToast(
                        'error',
                        snapshot.error?.message ?? 'Failed to save appearance preference',
                      );
                      stop();
                    } else if (
                      snapshot.status === 'ready' &&
                      snapshot.confirmed?.themePreference === option.value
                    ) {
                      stop();
                    } else if (
                      snapshot.status === 'ready' &&
                      snapshot.confirmed?.themePreference === previous
                    ) {
                      stop();
                    }
                  });
                }}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
        <div class={styles.body}>
          <OperationsConsole onSignedOut={() => navigate({ to: '/login' })} />
        </div>
        <button
          type="button"
          class={styles.hiddenClose}
          aria-label="Close Settings"
          onClick={() => setOpen(false)}
        >
          <X />
        </button>
      </Dialog>
    </>
  );
}
