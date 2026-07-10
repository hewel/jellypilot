import type { ThemePreference } from '@bindings';
import { Monitor, Moon, Sun } from 'lucide-solid';
import { createMemo } from 'solid-js';

import { useConfigCoordinator } from '../effects/configContext';
import { useToast } from './ToastProvider';

import * as styles from './ThemeCycleControl.css';

const ORDER: ThemePreference[] = ['system', 'light', 'dark'];

function nextPreference(current: ThemePreference): ThemePreference {
  const index = ORDER.indexOf(current);
  return ORDER[(index + 1) % ORDER.length]!;
}

function labelFor(preference: ThemePreference): string {
  if (preference === 'system') return 'System';
  if (preference === 'light') return 'Light';
  return 'Dark';
}

export default function ThemeCycleControl() {
  const { coordinator, state } = useConfigCoordinator();
  const toast = useToast();
  const preference = createMemo(
    () => state().desired?.themePreference ?? state().confirmed?.themePreference ?? 'system',
  );
  const next = createMemo(() => nextPreference(preference()));
  const accessibleLabel = createMemo(
    () => `Theme: ${labelFor(preference())}. Click for ${labelFor(next())}.`,
  );

  return (
    <button
      type="button"
      class={styles.control}
      aria-label={accessibleLabel()}
      title={accessibleLabel()}
      data-theme-preference={preference()}
      onClick={() => {
        const previous = preference();
        const upcoming = nextPreference(previous);
        coordinator.setThemePreference(upcoming);
        const stop = coordinator.subscribe((snapshot) => {
          if (snapshot.status === 'error') {
            toast.showToast('error', snapshot.error?.message ?? 'Failed to save theme preference');
            stop();
          } else if (
            snapshot.status === 'ready' &&
            snapshot.confirmed?.themePreference === upcoming
          ) {
            stop();
          }
        });
      }}
    >
      {preference() === 'system' ? (
        <Monitor class={styles.icon} aria-hidden="true" />
      ) : preference() === 'light' ? (
        <Sun class={styles.icon} aria-hidden="true" />
      ) : (
        <Moon class={styles.icon} aria-hidden="true" />
      )}
    </button>
  );
}
