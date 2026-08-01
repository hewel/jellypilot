import { $, browser, expect } from '@wdio/globals';

import type {
  AppConfig,
  ConnectionState,
  NowPlayingState,
  SavedServiceProfiles,
  VideoHome,
} from '../../src/bindings';

const connectedState = {
  capabilities: {
    introSkipper: true,
    quickConnect: true,
    remoteControl: false,
    remoteControlAvailable: false,
    remoteControlWarning: null,
  },
  connected: true,
  provider: 'jellyfin',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
} as const satisfies ConnectionState;

// The active profile also carries a restore error: the warning border must win.
const savedProfiles = {
  activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
  profiles: [
    {
      active: true,
      key: 'jellyfin|https://jellyfin.example.com|Ada',
      lastRestoreError: 'E2E restore failure',
      provider: 'jellyfin',
      reauthRequired: false,
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
  ],
} as const satisfies SavedServiceProfiles;

const appConfig = {
  deviceName: 'JellyPilot',
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
} as const satisfies AppConfig;

const videoHome = {
  continueWatching: [],
  nextUp: [],
  latestMovies: [],
  latestEpisodes: [],
} as const satisfies VideoHome;

const offlineState = {
  canPlayNext: false,
  canPlayPrevious: false,
  media: null,
  nextUnavailableReason: 'noCurrentItem',
  player: {
    connected: false,
    duration: 0,
    muted: false,
    paused: true,
    timePos: 0,
    volume: 100,
  },
  previousUnavailableReason: 'noCurrentItem',
  status: 'offline',
} as const satisfies NowPlayingState;

const fixtures = {
  server_is_connected: true,
  server_get_state: connectedState,
  server_profiles_get: savedProfiles,
  config_get: appConfig,
  mpv_is_connected: false,
  library_video_home: videoHome,
  library_video_shortcuts: [] as const,
  now_playing_get_state: offlineState,
} as const;

// primary #4f46e5; secondary #818cf8.
const PRIMARY = 'rgb(79, 70, 229)';
const SECONDARY = 'rgb(129, 140, 248)';

describe('Operations console merge semantics', () => {
  it('resolves checkbox, choice, profile precedence, and chevron state deterministically', async () => {
    await browser.waitUntil(
      () => browser.execute(() => window.__JELLYPILOT_E2E__?.ready === true),
      {
        timeout: 30_000,
        timeoutMsg: 'The controlled Tauri bridge did not become ready before mount.',
      },
    );
    await browser.execute((values: typeof fixtures) => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller?.mount) throw new Error('The E2E bridge mount was already consumed.');
      controller.installFixture('server_is_connected', {
        kind: 'return',
        value: values.server_is_connected,
      });
      controller.installFixture('server_get_state', {
        kind: 'return',
        value: values.server_get_state,
      });
      controller.installFixture('server_profiles_get', {
        kind: 'return',
        value: values.server_profiles_get,
      });
      controller.installFixture('config_get', {
        kind: 'return',
        value: values.config_get,
      });
      controller.installFixture('mpv_is_connected', {
        kind: 'return',
        value: values.mpv_is_connected,
      });
      controller.installFixture('library_video_home', {
        kind: 'return',
        value: values.library_video_home,
      });
      controller.installFixture('library_video_shortcuts', {
        kind: 'return',
        value: [...values.library_video_shortcuts],
      });
      controller.installFixture('now_playing_get_state', {
        kind: 'return',
        value: values.now_playing_get_state,
      });
      controller.mount();
    }, fixtures);

    const settings = await $('aria/Open Settings');
    await settings.waitForDisplayed({ timeout: 30_000 });
    await settings.click();

    const settingsDialog = await $('[data-part="content"]');
    await settingsDialog.waitForDisplayed({ timeout: 30_000 });

    // Library image cache checkbox: _groupChecked fills the box with primary.
    await browser.waitUntil(
      () =>
        browser.execute((expected) => {
          const toggle = document.querySelector(
            'button[role="checkbox"][aria-label="Library Image cache"]',
          );
          const box = toggle?.querySelector('span[aria-hidden="true"]');
          if (!toggle || !box) return false;
          return (
            toggle.getAttribute('aria-checked') === 'true' &&
            getComputedStyle(box).backgroundColor === expected
          );
        }, PRIMARY),
      {
        timeout: 10_000,
        timeoutMsg: 'Checked image cache box did not fill with the primary token.',
      },
    );

    // Intro Skip choice: _pressed paints the selected mode deterministically.
    const choice = await browser.execute(() => {
      const pressed = document.querySelector(
        'fieldset[aria-label="Intro Skip Mode"] button[aria-pressed="true"]',
      );
      if (!pressed) return null;
      const style = getComputedStyle(pressed);
      return { borderColor: style.borderColor, fontWeight: style.fontWeight };
    });
    expect(choice).not.toBeNull();
    expect(choice?.borderColor).toBe(PRIMARY);
    expect(choice?.fontWeight).toBe('600');

    // Saved services: the warning border overrides the active ring (merge order).
    // WebKit serializes color-mix borders as color(srgb …), Chromium as rgba(…).
    const profileBorder = await browser.execute(() => {
      const name = [...document.querySelectorAll('p')].find(
        (p) => p.textContent?.trim() === 'Jellyfin Home',
      );
      let node: Element | null = name ?? null;
      while (node) {
        const style = getComputedStyle(node);
        if (style.borderStyle === 'solid' && style.borderWidth === '1px') {
          return style.borderColor;
        }
        node = node.parentElement;
      }
      return 'missing-profile-row';
    });
    expect(
      /^(rgba\(246, 199, 104, 0\.6\)|color\(srgb 0\.9647\d* 0\.7803\d* 0\.4078\d* \/ 0\.6\))$/.test(
        profileBorder,
      ),
    ).toBe(true);

    // Advanced MPV chevron: [data-state=open] & rotates and recolors the icon.
    const advancedTrigger = await $('button*=Advanced MPV options');
    await advancedTrigger.waitForDisplayed({ timeout: 10_000 });
    const before = await browser.execute(() => {
      const trigger = [...document.querySelectorAll('button')].find((button) =>
        button.textContent?.includes('Advanced MPV options'),
      );
      const svg = trigger?.querySelector('svg');
      return svg ? getComputedStyle(svg).transform : 'missing-svg';
    });
    expect(before === 'none' || before === 'matrix(1, 0, 0, 1, 0, 0)').toBe(true);
    await advancedTrigger.click();
    await browser.waitUntil(
      () =>
        browser.execute((expectedColor) => {
          const trigger = [...document.querySelectorAll('button')].find((button) =>
            button.textContent?.includes('Advanced MPV options'),
          );
          const svg = trigger?.querySelector('svg');
          if (!svg) return false;
          const style = getComputedStyle(svg);
          return style.transform.startsWith('matrix') && style.color === expectedColor;
        }, SECONDARY),
      {
        timeout: 10_000,
        timeoutMsg: 'Advanced MPV chevron did not rotate and recolor on open.',
      },
    );
  });
});
