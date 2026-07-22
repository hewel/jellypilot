import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  NowPlayingState,
  VideoHome,
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryPage,
  VideoLibraryShortcut,
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

const moviesShortcut = {
  id: 'movies',
  name: 'Movies',
  collectionType: 'movies',
  itemCount: 96,
  artworkImageId: null,
} as const satisfies VideoLibraryShortcut;

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

const browseItems: VideoLibraryItem[] = Array.from({ length: 96 }, (_, index) => ({
  id: `e2e-movie-${index + 1}`,
  name: `E2E Movie ${index + 1}`,
  itemType: 'Movie',
  productionYear: null,
  runtimeSeconds: null,
  played: false,
  favorite: false,
  artworkImageId: null,
  seasonNumber: null,
  episodeNumber: null,
  seriesId: null,
  seriesName: null,
  resumePositionSeconds: null,
  playedPercentage: null,
}));

const browsePage = {
  collectionType: 'movies',
  libraryId: 'movies',
  startIndex: 0,
  limit: 96,
  totalRecordCount: 96,
  hasMore: false,
  items: browseItems,
} as const satisfies VideoLibraryPage;

const itemDetail = {
  id: 'e2e-detail-movie',
  name: 'E2E Detail Movie',
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
  playedPercentage: null,
  resumePositionSeconds: null,
  canResume: false,
  canPlay: false,
  artworkImageId: null,
  backdropImageId: null,
  audioStreams: [],
  subtitleStreams: [],
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
  library_video_shortcuts: [moviesShortcut],
  library_browse_video: browsePage,
  library_item_detail: itemDetail,
  library_item_shortcut: moviesShortcut,
  now_playing_get_state: offlineState,
} as const;

function appScrollViewportTop(): number {
  const viewport = document.querySelector<HTMLElement>('[data-testid="app-scroll-viewport"]');
  if (!viewport) throw new Error('App scroll viewport was not rendered');
  return viewport.scrollTop;
}

describe('library item detail Back restores origin and scroll', () => {
  it('returns to Home and Movies origins with nested scroll restoration in one app lifetime', async () => {
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
      controller.installFixture('library_browse_video', {
        kind: 'return',
        value: values.library_browse_video,
      });
      controller.installFixture('library_item_detail', {
        kind: 'return',
        value: values.library_item_detail,
      });
      controller.installFixture('library_item_shortcut', {
        kind: 'return',
        value: values.library_item_shortcut,
      });
      controller.installFixture('now_playing_get_state', {
        kind: 'return',
        value: values.now_playing_get_state,
      });
      controller.mount();
    }, fixtures);

    const sidebar = await $('aria/Sidebar');
    await sidebar.waitForDisplayed({ timeout: 30_000 });
    const continueWatching = await $('aria/Continue Watching');
    await continueWatching.waitForDisplayed({ timeout: 30_000 });
    expect(await browser.execute(() => window.location.pathname)).toBe('/library');

    // Home origin: Back returns to the library landing.
    const homeCard = await $('aria/Open E2E Home Movie');
    await homeCard.waitForDisplayed({ timeout: 30_000 });
    await homeCard.click();
    const detailHeading = await $('aria/E2E Detail Movie');
    await detailHeading.waitForDisplayed({ timeout: 30_000 });
    expect(await browser.execute(() => window.location.pathname)).toBe(
      '/library/items/e2e-home-movie',
    );

    const backFromHome = await $('aria/Back');
    await backFromHome.click();
    const homeHeading = await $('aria/Continue Watching');
    await homeHeading.waitForDisplayed({ timeout: 30_000 });
    expect(await browser.execute(() => window.location.pathname)).toBe('/library');

    // Movies origin: Back restores the nested viewport offset.
    const moviesLink = await sidebar.$('aria/Movies');
    await moviesLink.click();
    await browser.waitUntil(
      () => browser.execute(() => window.location.pathname === '/library/movies/movies'),
      {
        timeout: 30_000,
        timeoutMsg: 'Movies browse route did not load.',
      },
    );

    const lastCard = await $('aria/Open E2E Movie 96');
    await lastCard.waitForDisplayed({ timeout: 30_000 });
    await browser.execute(
      (element: HTMLElement) => {
        element.scrollIntoView({ block: 'end' });
      },
      lastCard as unknown as HTMLElement,
    );
    await browser.waitUntil(async () => (await browser.execute(appScrollViewportTop)) > 0, {
      timeoutMsg: 'Browsing to the last card did not scroll the app viewport.',
    });
    /* Scroll events dispatch asynchronously; nudge the listener so TanStack
     * snapshots this exact offset before the navigation below. */
    await browser.execute(() => {
      document
        .querySelector('[data-testid="app-scroll-viewport"]')
        ?.dispatchEvent(new Event('scroll'));
    });
    const savedScrollTop = await browser.execute(appScrollViewportTop);

    /* Click inside the page so WebDriver does not scroll the card into view
     * again and change the offset that Back must restore. */
    await browser.execute(
      (element: HTMLElement) => {
        element.click();
      },
      lastCard as unknown as HTMLElement,
    );
    const browseDetailHeading = await $('aria/E2E Detail Movie');
    await browseDetailHeading.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(async () => (await browser.execute(appScrollViewportTop)) === 0, {
      timeoutMsg: 'Navigating to item detail did not reset the app viewport to the top.',
    });

    const backFromMovies = await $('aria/Back');
    await backFromMovies.click();
    const moviesHeading = await $('aria/Movies');
    await moviesHeading.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(
      async () => Math.abs((await browser.execute(appScrollViewportTop)) - savedScrollTop) <= 1,
      {
        timeoutMsg: `Back did not restore the app viewport offset near ${savedScrollTop}.`,
      },
    );
  });
});
