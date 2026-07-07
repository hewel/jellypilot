import { afterEach, expect, rstest, test } from '@rstest/core';
import { Effect, Exit } from 'effect';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import type { AppConfig } from '../src/bindings';
import { saveThemePreference } from '../src/effects/config';
import {
  ThemeSync,
  applyDocumentTheme,
  normalizeThemePreference,
  resolveThemePreference,
} from '../src/theme';

const config: AppConfig = {
  deviceName: 'JellyPilot Test',
  imageDiskCacheEnabled: true,
  introSkipperMode: 'automatic',
  keybindIntroSkip: 'g',
  keybindNext: 'Shift+>',
  keybindPrev: 'Shift+<',
  mpvArgs: [],
  mpvPath: null,
  preferredSubtitleLanguages: [],
  progressInterval: 5,
  startMinimized: false,
  themePreference: 'system',
};

type MediaQueryChangeListener = (this: MediaQueryList, event: MediaQueryListEvent) => void;

function createTestMediaQueryList(initialMatches: boolean) {
  const listeners = new Set<MediaQueryChangeListener>();
  const mediaQuery = {
    matches: initialMatches,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: (type: string, listener: EventListenerOrEventListenerObject) => {
      if (type === 'change' && typeof listener === 'function') {
        listeners.add(listener as MediaQueryChangeListener);
      }
    },
    removeEventListener: (type: string, listener: EventListenerOrEventListenerObject) => {
      if (type === 'change' && typeof listener === 'function') {
        listeners.delete(listener as MediaQueryChangeListener);
      }
    },
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => true,
  } as MediaQueryList;

  return {
    mediaQuery,
    setMatches(matches: boolean) {
      Object.defineProperty(mediaQuery, 'matches', {
        configurable: true,
        value: matches,
      });
      const event = new Event('change') as MediaQueryListEvent;
      Object.defineProperty(event, 'matches', {
        configurable: true,
        value: matches,
      });
      for (const listener of listeners) {
        listener.call(mediaQuery, event);
      }
    },
  };
}

afterEach(() => {
  rstest.restoreAllMocks();
  delete document.documentElement.dataset.theme;
  document.documentElement.style.colorScheme = '';
  document.body.innerHTML = '';
});

test('theme preference normalization falls back to system', () => {
  expect(normalizeThemePreference('dark')).toBe('dark');
  expect(normalizeThemePreference(undefined)).toBe('system');
  expect(resolveThemePreference('system', true)).toBe('dark');
  expect(resolveThemePreference('system', false)).toBe('light');
  expect(resolveThemePreference('light', true)).toBe('light');
});

test('document theme application updates root data attributes', () => {
  applyDocumentTheme('dark');

  expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  expect(document.documentElement.style.colorScheme).toBe('dark');
});

test('saveThemePreference persists through typed config workflow', async () => {
  rstest.spyOn(commands, 'configGet').mockResolvedValue(config);
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });

  const exit = await Effect.runPromiseExit(saveThemePreference('dark'));

  expect(Exit.isSuccess(exit)).toBe(true);
  expect(configSet).toHaveBeenCalledWith({
    ...config,
    themePreference: 'dark',
  });
});

test('ThemeSync applies saved system preference and follows OS changes', async () => {
  const mediaQuery = createTestMediaQueryList(false);
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: () => mediaQuery.mediaQuery,
  });
  rstest.spyOn(commands, 'configGet').mockResolvedValue({
    ...config,
    themePreference: 'system',
  });
  const root = document.createElement('div');
  document.body.append(root);

  const dispose = render(() => <ThemeSync />, root);

  expect(document.documentElement).toHaveAttribute('data-theme', 'light');
  mediaQuery.setMatches(true);
  expect(document.documentElement).toHaveAttribute('data-theme', 'dark');

  dispose();
});
