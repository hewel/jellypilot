import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  EmbeddedPlayerState,
  NowPlayingState,
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

const activePlayer = {
  canPlayInMpv: true,
  desiredMuted: false,
  desiredPaused: true,
  desiredSeekPositionSeconds: null,
  desiredVolume: 80,
  durationSeconds: 300,
  dynamicRange: 'sdr',
  failure: null,
  generation: 3,
  itemId: 'movie-1',
  phase: 'preparing',
  playlistUrl: null,
  positionSeconds: 45,
  revision: 1,
  sessionId: 'embedded-e2e-session',
  subtitle: null,
  timelineOffsetSeconds: 40,
  title: 'Embedded E2E Movie',
  videoCodec: 'h264',
} as const satisfies EmbeddedPlayerState;

const stoppedPlayer = {
  ...activePlayer,
  phase: 'stopped',
  playlistUrl: null,
  revision: 2,
} as const satisfies EmbeddedPlayerState;

const emptyVideoHome = {
  continueWatching: [],
  latestEpisodes: [],
  latestMovies: [],
  nextUp: [],
} as const satisfies VideoHome;

const offlineNowPlaying = {
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
  activePlayer,
  connectedState,
  emptyVideoHome,
  offlineNowPlaying,
  stoppedPlayer,
} as const;

describe('Embedded player route', () => {
  it('routes an active native session into immersive chrome and stops through typed IPC', async () => {
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
        value: true,
      });
      controller.installFixture('server_get_state', {
        kind: 'return',
        value: values.connectedState,
      });
      controller.installFixture('library_video_home', {
        kind: 'return',
        value: values.emptyVideoHome,
      });
      controller.installFixture('library_video_shortcuts', {
        kind: 'return',
        value: [],
      });
      controller.installFixture('now_playing_get_state', {
        kind: 'return',
        value: values.offlineNowPlaying,
      });
      controller.installFixture('embedded_player_get_state', {
        kind: 'return',
        value: values.activePlayer,
      });
      controller.installFixture('embedded_player_register_capabilities', {
        kind: 'return',
        value: values.activePlayer,
      });
      controller.installFixture('embedded_player_observe', {
        kind: 'return',
        value: values.activePlayer,
      });
      controller.installFixture('embedded_player_control', {
        kind: 'return',
        value: values.stoppedPlayer,
      });
      controller.mount();
    }, fixtures);

    const title = await $('aria/Embedded E2E Movie');
    await title.waitForDisplayed({ timeout: 30_000 });
    expect(await browser.execute(() => window.location.pathname)).toBe('/player');
    expect(await $('[data-shell]').isExisting()).toBe(false);

    const close = await $('aria/Stop playback and close player');
    expect(await close.isDisplayed()).toBe(true);
    await browser.execute(() => {
      const element = document.querySelector<HTMLButtonElement>(
        'button[aria-label="Stop playback and close player"]',
      );
      if (!element) throw new Error('The accessible player close control was not found.');
      window.setTimeout(() => element.click(), 0);
    });
    await browser.waitUntil(
      async () => (await browser.execute(() => window.location.pathname)) === '/library',
      {
        timeout: 30_000,
        timeoutMsg: 'Stopping embedded playback did not leave the immersive route.',
      },
    );
    expect(
      await browser.execute(
        () => window.__JELLYPILOT_E2E__?.callCount('embedded_player_control') ?? 0,
      ),
    ).toBe(1);
  });
});
