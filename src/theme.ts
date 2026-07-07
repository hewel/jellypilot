import type { ThemePreference } from '@bindings';
import { Exit } from 'effect';
import { createEffect, createSignal, onCleanup, onMount } from 'solid-js';

import { fetchConfig } from './effects/config';
import { runExit } from './effects/query';

export type ResolvedTheme = 'light' | 'dark';

const themePreferences = new Set<ThemePreference>(['system', 'light', 'dark']);

export function normalizeThemePreference(
  preference: ThemePreference | null | undefined,
): ThemePreference {
  return preference && themePreferences.has(preference) ? preference : 'system';
}

export function resolveThemePreference(
  preference: ThemePreference | null | undefined,
  systemPrefersDark: boolean,
): ResolvedTheme {
  const normalized = normalizeThemePreference(preference);
  if (normalized === 'light') {
    return 'light';
  }
  if (normalized === 'dark') {
    return 'dark';
  }
  return systemPrefersDark ? 'dark' : 'light';
}

export function applyDocumentTheme(theme: ResolvedTheme, root = document.documentElement) {
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
}

export function ThemeSync() {
  const mediaQuery =
    typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-color-scheme: dark)')
      : null;
  const [themePreference, setThemePreference] = createSignal<ThemePreference>('system');
  const [systemPrefersDark, setSystemPrefersDark] = createSignal(mediaQuery?.matches ?? false);

  const updateSystemPreference = (event: MediaQueryListEvent) => {
    setSystemPrefersDark(event.matches);
  };

  onMount(() => {
    void runExit(fetchConfig()).then((exit) => {
      if (Exit.isSuccess(exit)) {
        setThemePreference(normalizeThemePreference(exit.value.themePreference));
      }
    });

    mediaQuery?.addEventListener('change', updateSystemPreference);
  });

  onCleanup(() => {
    mediaQuery?.removeEventListener('change', updateSystemPreference);
  });

  createEffect(() => {
    applyDocumentTheme(resolveThemePreference(themePreference(), systemPrefersDark()));
  });

  return null;
}
