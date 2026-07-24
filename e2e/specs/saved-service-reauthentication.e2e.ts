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
    remoteControl: true,
    remoteControlAvailable: true,
    remoteControlWarning: null,
  },
  connected: true,
  provider: 'jellyfin',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
} as const satisfies ConnectionState;

const embyProfileKey = 'emby|https://media.example.com/emby|Grace';

const savedProfiles = {
  activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
  profiles: [
    {
      active: true,
      key: 'jellyfin|https://jellyfin.example.com|Ada',
      lastRestoreError: null,
      provider: 'jellyfin',
      reauthRequired: false,
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
    {
      active: false,
      key: embyProfileKey,
      lastRestoreError: null,
      provider: 'emby',
      reauthRequired: false,
      serverName: 'Emby Home',
      serverUrl: 'https://media.example.com/emby',
      userName: 'Grace',
    },
  ],
} as const satisfies SavedServiceProfiles;

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

const fixtures = {
  server_is_connected: true,
  server_get_state: connectedState,
  server_profiles_get: savedProfiles,
  config_get: appConfig,
  mpv_is_connected: false,
  library_video_home: videoHome,
  library_video_shortcuts: [],
  now_playing_get_state: offlineState,
} as const;

describe('Saved service reauthentication', () => {
  it('recovers an expired Emby profile through the locked sign-in dialog', async () => {
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
      controller.installFixture('server_profiles_activate', {
        kind: 'error',
        error: { code: 'authFailed', message: 'expired' },
      });
      controller.installFixture('server_profiles_reauthenticate_password', {
        kind: 'return',
        value: {
          activeProfileKey: 'emby|https://media.example.com/emby|Grace',
          profiles: [
            { ...values.server_profiles_get.profiles[0], active: false },
            { ...values.server_profiles_get.profiles[1], active: true },
          ],
        },
      });
      controller.mount();
    }, fixtures);

    const settings = await $('aria/Open Settings');
    await settings.waitForDisplayed({ timeout: 30_000 });
    await settings.click();

    const settingsDialog = await $('[data-part="content"]');
    await settingsDialog.waitForDisplayed({ timeout: 30_000 });

    const activate = await settingsDialog.$('button=Activate');
    await activate.waitForDisplayed({ timeout: 30_000 });
    await activate.click();

    const password = await $('[placeholder="Jellyfin password"]');
    await password.waitForDisplayed({
      timeout: 30_000,
      timeoutMsg: 'Failed activation did not open the locked sign-in dialog.',
    });
    const dialogBounds = await browser.execute(() => {
      const input = document.querySelector('[placeholder="Jellyfin password"]');
      const positioner = input?.closest('[data-part="positioner"]');
      if (!positioner) return null;
      const rect = positioner.getBoundingClientRect();
      return {
        top: rect.top,
        left: rect.left,
        width: rect.width,
        height: rect.height,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      };
    });
    expect(dialogBounds).not.toBeNull();
    expect(Math.abs(dialogBounds?.top ?? 0)).toBeLessThanOrEqual(2);
    expect(Math.abs(dialogBounds?.left ?? 0)).toBeLessThanOrEqual(2);
    expect(dialogBounds?.width).toBeGreaterThanOrEqual((dialogBounds?.viewportWidth ?? 0) - 2);
    expect(dialogBounds?.height).toBeGreaterThanOrEqual((dialogBounds?.viewportHeight ?? 0) - 2);
    const dialogText = await browser.execute(() => document.body.textContent ?? '');
    expect(dialogText).toContain('Sign in again');
    expect(dialogText).toContain(
      'Your saved session expired. Sign in again to switch to this service.',
    );
    expect(dialogText).toContain('Emby Home');
    expect(dialogText).toContain('Grace');
    await password.setValue('not-a-secret');

    const submit = await $('button=Sign in and switch');
    await submit.click();

    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            window.__JELLYPILOT_E2E__?.callCount('server_profiles_reauthenticate_password') === 1,
        ),
      {
        timeout: 30_000,
        timeoutMsg: 'Reauthentication did not invoke the password command exactly once.',
      },
    );
    expect(
      await browser.execute(
        (key: string) => window.__JELLYPILOT_E2E__?.hasExpectedReauthenticatePasswordCall(key),
        embyProfileKey,
      ),
    ).toBe(true);

    await password.waitForDisplayed({ reverse: true, timeout: 30_000 });

    const toast = await browser.waitUntil(
      async () => {
        const text = await browser.execute(() => document.body.textContent ?? '');
        return text.includes('Signed in and switched service');
      },
      {
        timeout: 30_000,
        timeoutMsg: 'The switched-service confirmation toast did not appear.',
      },
    );
    expect(toast).toBe(true);
  });
});
