import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  NowPlayingState,
  VideoHome,
  VideoItemDetail,
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

const videoHome = {
  continueWatching: [
    {
      id: 'e2e-home-movie',
      name: 'E2E Home Movie',
      itemType: 'Movie',
      seriesId: null,
      seriesName: null,
      seasonNumber: null,
      episodeNumber: null,
      productionYear: 2024,
      runtimeSeconds: 7200,
      resumePositionSeconds: 120,
      playedPercentage: 25,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
  ],
  nextUp: [],
  latestMovies: [],
  latestEpisodes: [],
} as const satisfies VideoHome;

const itemDetail = {
  id: 'e2e-home-movie',
  name: 'E2E Home Movie',
  itemType: 'Movie',
  overview: null,
  productionYear: 2024,
  runtimeSeconds: 7200,
  seriesId: null,
  seriesName: null,
  seasonNumber: null,
  episodeNumber: null,
  genres: [],
  played: false,
  favorite: false,
  playedPercentage: 25,
  resumePositionSeconds: 120,
  canResume: true,
  canPlay: true,
  artworkImageId: null,
  backdropImageId: null,
} as const satisfies VideoItemDetail;

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
  library_video_home: videoHome,
  library_video_shortcuts: [],
  library_item_detail: itemDetail,
  library_item_streams: {
    audioStreams: [
      {
        codec: 'aac',
        index: 1,
        isDefault: true,
        isExternal: false,
        label: 'English AAC',
        language: 'eng',
      },
    ],
    subtitleStreams: [],
  },
  library_play: null,
  now_playing_get_state: offlineState,
} as const;

describe('Video Home direct resume', () => {
  it('sends saved resume state and keeps the first detail load visible', async () => {
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
      controller.installFixture('library_video_home', {
        kind: 'return',
        value: values.library_video_home,
      });
      controller.installFixture('library_video_shortcuts', {
        kind: 'return',
        value: [...values.library_video_shortcuts],
      });
      controller.installFixture('library_item_detail', {
        kind: 'return',
        value: values.library_item_detail,
      });
      controller.installFixture('library_item_streams', {
        kind: 'return',
        delayMs: 3000,
        value: {
          audioStreams: [...values.library_item_streams.audioStreams],
          subtitleStreams: [...values.library_item_streams.subtitleStreams],
        },
      });
      controller.installFixture('library_play', {
        kind: 'return',
        value: values.library_play,
      });
      controller.installFixture('now_playing_get_state', {
        kind: 'return',
        value: values.now_playing_get_state,
      });
      controller.mount();
    }, fixtures);

    const resume = await $('aria/Resume E2E Home Movie');
    await resume.waitForDisplayed({ timeout: 30_000 });
    await resume.click();

    await browser.waitUntil(
      () => browser.execute(() => window.__JELLYPILOT_E2E__?.callCount('library_play') === 1),
      {
        timeout: 30_000,
        timeoutMsg: 'Video Home did not invoke library_play exactly once.',
      },
    );
    expect(
      await browser.execute(() => window.__JELLYPILOT_E2E__?.hasExpectedLibraryPlayCall()),
    ).toBe(true);
    expect(await browser.execute(() => window.location.pathname)).toBe('/library');

    const detailLink = await $('a[href="/library/items/e2e-home-movie"]');
    await detailLink.click();
    const detailHeading = await $('aria/E2E Home Movie');
    await detailHeading.waitForDisplayed({ timeout: 1000 });

    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('eng') === true),
      {
        timeout: 5000,
        timeoutMsg: 'Deferred audio metadata did not render after the detail page became usable.',
      },
    );

    const back = await $('aria/Back');
    await back.click();
    await browser.waitUntil(
      async () => (await browser.execute(() => window.location.pathname)) === '/library',
      { timeout: 5000, timeoutMsg: 'Detail back navigation did not return to the library.' },
    );

    const cachedDetailLink = await $('a[href="/library/items/e2e-home-movie"]');
    await cachedDetailLink.click();
    const cachedDetailHeading = await $('aria/E2E Home Movie');
    await cachedDetailHeading.waitForDisplayed({ timeout: 5000 });
  });
});
