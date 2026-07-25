import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  NowPlayingState,
  VideoHome,
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryPage,
  VideoLibraryShortcut,
  VideoSearchPage,
} from '../../src/bindings';

const SEARCH_QUERY = 'Neon Reef';
const FINAL_RESULT_NAME = 'E2E Search Finale';

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

const browsePage = {
  collectionType: 'movies',
  libraryId: 'movies',
  startIndex: 0,
  limit: 24,
  totalRecordCount: 1,
  hasMore: false,
  items: [
    {
      id: 'e2e-browse-movie',
      name: 'E2E Browse Movie',
      itemType: 'Movie',
      productionYear: 2024,
      runtimeSeconds: 6600,
      played: false,
      favorite: false,
      artworkImageId: null,
      seasonNumber: null,
      episodeNumber: null,
      seriesId: null,
      seriesName: null,
      resumePositionSeconds: null,
      playedPercentage: null,
    },
  ],
} as const satisfies VideoLibraryPage;

const searchItems: VideoLibraryItem[] = [
  ...Array.from({ length: 23 }, (_, index) => ({
    id: `e2e-search-result-${index + 1}`,
    name: `E2E Search Result ${String(index + 1).padStart(2, '0')}`,
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
  })),
  {
    id: 'e2e-search-finale',
    name: FINAL_RESULT_NAME,
    itemType: 'Movie',
    productionYear: 2024,
    runtimeSeconds: 6600,
    played: false,
    favorite: false,
    artworkImageId: null,
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
];

const searchPage = {
  query: SEARCH_QUERY,
  startIndex: 0,
  limit: 24,
  totalRecordCount: 24,
  hasMore: false,
  items: searchItems,
} as const satisfies VideoSearchPage;

const finaleDetail = {
  id: 'e2e-search-finale',
  name: FINAL_RESULT_NAME,
  itemType: 'Movie',
  overview: null,
  productionYear: 2024,
  runtimeSeconds: 6600,
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
  library_search_video: searchPage,
  library_item_detail: finaleDetail,
  library_item_shortcut: moviesShortcut,
  now_playing_get_state: offlineState,
} as const;

function appScrollViewportTop(): number {
  const viewport = document.querySelector<HTMLElement>('[data-testid="app-scroll-viewport"]');
  if (!viewport) throw new Error('App scroll viewport was not rendered');
  return viewport.scrollTop;
}

describe('library search finds results beyond home rows and restores them on Back', () => {
  it('submits from the shell bar, opens the final result, and restores row and scroll', async () => {
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
      controller.installFixture('library_search_video', {
        kind: 'return',
        value: values.library_search_video,
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
    expect(await browser.execute(() => window.location.pathname)).toBe('/library');

    // The shell search bar lives inside the Library browse toolbar.
    const moviesLink = await sidebar.$('aria/Movies');
    await moviesLink.click();
    await browser.waitUntil(
      () => browser.execute(() => window.location.pathname === '/library/movies/movies'),
      {
        timeout: 30_000,
        timeoutMsg: 'Movies browse route did not load.',
      },
    );

    // Submit the query from the browse toolbar search bar.
    const searchInput = await $('form[role="search"] input[aria-label="Search library"]');
    await searchInput.waitForDisplayed({ timeout: 30_000 });
    await searchInput.click();
    await searchInput.setValue(SEARCH_QUERY);
    const submitSearch = await $('form[role="search"] button[type="submit"]');
    await submitSearch.click();
    await browser.waitUntil(
      () => browser.execute(() => window.location.pathname === '/library/search'),
      {
        timeout: 30_000,
        timeoutMsg: 'Submitting the shell search did not navigate to /library/search.',
      },
    );
    expect(await browser.execute(() => window.location.search)).toContain('q=Neon');

    const finaleRow = await $(`aria/Open ${FINAL_RESULT_NAME}`);
    await finaleRow.waitForDisplayed({ timeout: 30_000 });
    await browser.execute(
      (element: HTMLElement) => {
        element.scrollIntoView({ block: 'end' });
      },
      finaleRow as unknown as HTMLElement,
    );
    await browser.waitUntil(async () => (await browser.execute(appScrollViewportTop)) > 0, {
      timeoutMsg: 'Scrolling to the final search result did not move the app viewport.',
    });
    /* Scroll events dispatch asynchronously; nudge the listener so TanStack
     * snapshots this exact offset before the navigation below. */
    await browser.execute(() => {
      document
        .querySelector('[data-testid="app-scroll-viewport"]')
        ?.dispatchEvent(new Event('scroll'));
    });
    const savedScrollTop = await browser.execute(appScrollViewportTop);

    /* Click inside the page so WebDriver does not scroll the row into view
     * again and change the offset that Back must restore. */
    await browser.execute(
      (element: HTMLElement) => {
        element.click();
      },
      finaleRow as unknown as HTMLElement,
    );
    const detailHeading = await $(`aria/${FINAL_RESULT_NAME}`);
    await detailHeading.waitForDisplayed({ timeout: 30_000 });
    expect(await browser.execute(() => window.location.pathname)).toBe(
      '/library/items/e2e-search-finale',
    );
    await browser.waitUntil(async () => (await browser.execute(appScrollViewportTop)) === 0, {
      timeoutMsg: 'Navigating to item detail did not reset the app viewport to the top.',
    });

    const backFromDetail = await $('aria/Back');
    await backFromDetail.click();
    await browser.waitUntil(
      () => browser.execute(() => window.location.pathname === '/library/search'),
      {
        timeout: 30_000,
        timeoutMsg: 'Back did not return to the search results route.',
      },
    );
    const restoredRow = await $(`aria/Open ${FINAL_RESULT_NAME}`);
    await restoredRow.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(
      async () => Math.abs((await browser.execute(appScrollViewportTop)) - savedScrollTop) <= 1,
      {
        timeoutMsg: `Back did not restore the app viewport offset near ${savedScrollTop}.`,
      },
    );
  });
});
