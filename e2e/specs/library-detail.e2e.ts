import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  NowPlayingState,
  VideoHome,
  VideoItemDetail,
  VideoItemStreams,
  VideoLibraryItem,
  VideoSeasonEpisodes,
  VideoUserDataUpdate,
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

const movieDetail = {
  id: 'e2e-detail-movie',
  name: 'E2E Detail Movie',
  itemType: 'Movie',
  overview:
    'A rich cinematic experience spanning galaxies and generations. ' +
    'The story follows a crew of explorers who discover an ancient signal ' +
    'from beyond the known universe, leading them on a journey that will ' +
    'test the limits of human endurance and reshape the future of civilization. ' +
    'With breathtaking visuals and a haunting score, this epic redefines the genre.',
  productionYear: 2024,
  runtimeSeconds: 7200,
  seriesId: null,
  seriesName: null,
  seasonNumber: null,
  episodeNumber: null,
  genres: ['Sci-Fi', 'Adventure'],
  played: false,
  favorite: false,
  playedPercentage: 25,
  resumePositionSeconds: 1800,
  canResume: true,
  canPlay: true,
  artworkImageId: 'poster-e2e',
  backdropImageId: 'backdrop-e2e',
  metadata: {
    communityRating: 8.7,
    officialRating: 'PG-13',
    creators: ['Director A', 'Director B', 'Director C'],
    cast: ['Actor 1', 'Actor 2', 'Actor 3', 'Actor 4', 'Actor 5', 'Actor 6'],
  },
} as const satisfies VideoItemDetail;

const itemStreams = {
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
} as const satisfies VideoItemStreams;

const recommendations: VideoLibraryItem[] = [
  {
    id: 'rec-1',
    name: 'Rec Movie One',
    itemType: 'Movie',
    overview: null,
    productionYear: 2023,
    runtimeSeconds: 5400,
    played: true,
    favorite: false,
    artworkImageId: null,
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  {
    id: 'rec-2',
    name: 'Rec Show Two',
    itemType: 'Series',
    overview: null,
    productionYear: 2022,
    runtimeSeconds: null,
    played: false,
    favorite: true,
    artworkImageId: null,
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  {
    id: 'rec-3',
    name: 'Rec Movie Three',
    itemType: 'Movie',
    overview: null,
    productionYear: 2021,
    runtimeSeconds: 6000,
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
  {
    id: 'rec-4',
    name: 'Rec Show Four',
    itemType: 'Series',
    overview: null,
    productionYear: 2020,
    runtimeSeconds: null,
    played: true,
    favorite: true,
    artworkImageId: null,
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  {
    id: 'rec-5',
    name: 'Rec Movie Five',
    itemType: 'Movie',
    overview: null,
    productionYear: 2019,
    runtimeSeconds: 4800,
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
  {
    id: 'rec-6',
    name: 'Rec Show Six',
    itemType: 'Series',
    overview: null,
    productionYear: 2018,
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
  },
  {
    id: 'rec-7',
    name: 'Rec Movie Seven',
    itemType: 'Movie',
    overview: null,
    productionYear: 2017,
    runtimeSeconds: 5100,
    played: true,
    favorite: false,
    artworkImageId: null,
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  {
    id: 'rec-8',
    name: 'Rec Show Eight',
    itemType: 'Series',
    overview: null,
    productionYear: 2016,
    runtimeSeconds: null,
    played: false,
    favorite: true,
    artworkImageId: null,
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
];

const emptySeasonEpisodes = {
  seriesId: '',
  seasonId: null,
  seasonNumber: null,
  episodes: [],
} as const satisfies VideoSeasonEpisodes;

const userDataUpdate = {
  itemId: 'e2e-detail-movie',
  played: false,
  favorite: true,
} as const satisfies VideoUserDataUpdate;

const emptyVideoHome = {
  continueWatching: [],
  nextUp: [],
  latestMovies: [],
  latestEpisodes: [],
} as const satisfies VideoHome;

const fixtures = {
  server_is_connected: true,
  server_get_state: connectedState,
  library_video_home: emptyVideoHome,
  library_video_shortcuts: [] as const,
  library_item_detail: movieDetail,
  library_item_streams: itemStreams,
  library_similar_video: recommendations,
  library_season_episodes: emptySeasonEpisodes,
  library_update_user_data: userDataUpdate,
  library_play: null,
  now_playing_get_state: offlineNowPlaying,
} as const;

/**
 * Prove the page has no horizontal overflow. The application content region is
 * the page scroll viewport; inner horizontal scrollers (the recommendation
 * carousel) are contained and intentionally scroll, so only the viewport's own
 * scrollWidth counts — matching the plan's `scrollWidth <= clientWidth` rule.
 */
function findOverflowOffenders(): Promise<string[]> {
  return browser.execute(() => {
    const viewport = document.querySelector<HTMLElement>(
      '[role="region"][aria-label="Application content"]',
    );
    if (!viewport) return ['no-viewport'];
    if (viewport.scrollWidth <= viewport.clientWidth + 1) return [];
    return [`viewport sw=${viewport.scrollWidth} cw=${viewport.clientWidth}`];
  });
}

/**
 * Prove the hero action buttons (primary play/resume, favorite, More actions)
 * remain on-screen and reachable. Accessible names come from aria-label when
 * present, otherwise text content — the primary and favorite buttons carry no
 * literal aria-label matching their visible text.
 */
function heroActionsReachable(): Promise<boolean> {
  return browser.execute(() => {
    const nameOf = (el: Element): string => {
      const labelled = el.getAttribute('aria-label');
      if (labelled && labelled.trim()) return labelled.trim();
      return (el.textContent ?? '').replaceAll(/\s+/g, ' ').trim();
    };
    const buttons = [...document.querySelectorAll('button')];
    const find = (test: (name: string) => boolean): Element | undefined =>
      buttons.find((button) => test(nameOf(button)));
    const primary = find((name) => name === 'Resume' || name === 'Play');
    const favorite = find(
      (name) => name === 'Add to favorites' || name === 'Remove from favorites',
    );
    const more = find((name) => name === 'More actions');
    const targets = [primary, favorite, more].filter(
      (button): button is Element => button !== undefined,
    );
    if (targets.length < 3) return false;
    for (const button of targets) {
      const rect = button.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return false;
      if (button.getAttribute('aria-hidden') === 'true') return false;
    }
    return true;
  });
}

/**
 * Resize to an exact CSS-pixel viewport. WebDriver's `setWindowSize` takes
 * device pixels, so on a HiDPI display (devicePixelRatio > 1) a raw call lands
 * at `width / dpr` CSS pixels — far below the intended breakpoint. Scale by the
 * live devicePixelRatio and pause for the WebView to reflow before any
 * assertion reads the layout.
 */
async function setCssViewport(cssWidth: number, cssHeight: number): Promise<void> {
  const dpr = await browser.execute(() => window.devicePixelRatio);
  await browser.setWindowSize(Math.round(cssWidth * dpr), Math.round(cssHeight * dpr));
  await browser.pause(800);
}

describe('Library detail page redesign', () => {
  it('renders hero metadata, synopsis toggle, overflow menu, recommendations, and responsive layout', async () => {
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
        value: values.library_item_streams,
      });
      controller.installFixture('library_similar_video', {
        kind: 'return',
        value: values.library_similar_video,
      });
      controller.installFixture('library_season_episodes', {
        kind: 'return',
        value: values.library_season_episodes,
      });
      controller.installFixture('library_update_user_data', {
        kind: 'return',
        value: values.library_update_user_data,
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

    // Navigate to the detail page.
    await browser.execute(() => {
      window.history.pushState({}, '', '/library/items/e2e-detail-movie');
      window.dispatchEvent(new PopStateEvent('popstate'));
    });

    // --- Hero metadata ---
    const heading = await $('aria/E2E Detail Movie');
    await heading.waitForDisplayed({ timeout: 30_000 });

    // Rating + content chips.
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('8.7/10') === true),
      { timeout: 10_000, timeoutMsg: 'Community rating chip did not render.' },
    );
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('PG-13') === true),
      { timeout: 5000, timeoutMsg: 'Official rating chip did not render.' },
    );

    // Creator truncation: 2 visible + "+1 more".
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('+1 more') === true),
      { timeout: 5000, timeoutMsg: 'Creator +N more marker did not render.' },
    );

    // Cast truncation: 4 visible + "+2 more".
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('+2 more') === true),
      { timeout: 5000, timeoutMsg: 'Cast +N more marker did not render.' },
    );

    // --- Resume progress + remaining minutes ---
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('90 min remaining') === true),
      { timeout: 5000, timeoutMsg: 'Remaining minutes did not render.' },
    );

    // --- Synopsis More/Less toggle ---
    const moreButton = await $('aria/More');
    await moreButton.waitForDisplayed({ timeout: 10_000 });
    await moreButton.click();
    const lessButton = await $('aria/Less');
    await lessButton.waitForDisplayed({ timeout: 5000 });
    await lessButton.click();
    const moreAgain = await $('aria/More');
    await moreAgain.waitForDisplayed({ timeout: 5000 });

    // --- Overflow menu: Favorite visible, start-over in More actions ---
    const favoriteBtn = await $('aria/Favorite');
    await favoriteBtn.waitForDisplayed({ timeout: 5000 });

    const moreActions = await $('aria/More actions');
    await moreActions.waitForDisplayed({ timeout: 5000 });
    await moreActions.click();
    const startOver = await $('aria/Play from beginning');
    await startOver.waitForDisplayed({ timeout: 5000 });
    const markPlayed = await $('aria/Mark played');
    await markPlayed.waitForDisplayed({ timeout: 5000 });

    // Close the menu by pressing Escape.
    await browser.keys(['Escape']);

    // --- Recommendation cards route by item type ---
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('Rec Movie One') === true),
      { timeout: 10_000, timeoutMsg: 'Recommendation shelf did not render.' },
    );

    // Movie card routes to /library/items/.
    const movieCardLink = await $('a[href="/library/items/rec-1"]');
    await movieCardLink.waitForExist({ timeout: 5000 });

    // Series card routes to /library/shows/.
    const seriesCardLink = await $('a[href="/library/shows/rec-2"]');
    await seriesCardLink.waitForExist({ timeout: 5000 });

    // Played/Favorite badges on recommendation cards.
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('Played') === true),
      { timeout: 5000, timeoutMsg: 'Played badge did not render on recommendation card.' },
    );
    await browser.waitUntil(
      () => browser.execute(() => document.body.textContent?.includes('Favorite') === true),
      { timeout: 5000, timeoutMsg: 'Favorite badge did not render on recommendation card.' },
    );

    // --- No real playback escape ---
    expect(
      await browser.execute(() => window.__JELLYPILOT_E2E__?.callCount('library_play') ?? 0),
    ).toBe(0);

    // --- Responsive layout checks (CSS-pixel widths) ---
    const widths: readonly (readonly [number, number])[] = [
      [1600, 900],
      [1280, 720],
      [800, 600],
      [640, 720],
      [360, 720],
    ];

    for (const [w, h] of widths) {
      await setCssViewport(w, h);

      // No horizontal overflow on the page scroll viewport.
      await browser.waitUntil(
        async () => {
          const offenders = await findOverflowOffenders();
          return offenders.length === 0;
        },
        {
          timeout: 5000,
          timeoutMsg: `Horizontal overflow detected at ${w}x${h}.`,
        },
      );

      // Hero actions remain keyboard-reachable.
      expect(await heroActionsReachable()).toBe(true);
    }

    // At 1280x720 the next section heading must intersect the viewport
    // (compactness contract).
    await setCssViewport(1280, 720);
    await browser.waitUntil(
      () =>
        browser.execute(() => {
          const headings = [...document.querySelectorAll('h2')];
          return headings.some((heading) => {
            const rect = heading.getBoundingClientRect();
            return rect.top < window.innerHeight && rect.bottom > 0;
          });
        }),
      {
        timeout: 5000,
        timeoutMsg: 'No section heading intersected the viewport at 1280x720.',
      },
    );

    // At 1280 and 800 the poster should be displayed.
    for (const w of [1280, 800]) {
      await setCssViewport(w, w === 800 ? 600 : 720);
      await browser.waitUntil(
        () =>
          browser.execute(() => {
            const poster = document.querySelector('[data-detail-poster]');
            if (!poster) return false;
            const style = getComputedStyle(poster);
            return style.display !== 'none' && style.visibility !== 'hidden';
          }),
        { timeout: 5000, timeoutMsg: `Poster not displayed at ${w}px viewport.` },
      );
    }

    // At 640 and 360 the poster should NOT be displayed.
    for (const w of [640, 360]) {
      await setCssViewport(w, 720);
      await browser.waitUntil(
        () =>
          browser.execute(() => {
            const poster = document.querySelector('[data-detail-poster]');
            if (!poster) return true;
            const style = getComputedStyle(poster);
            return style.display === 'none' || style.visibility === 'hidden';
          }),
        { timeout: 5000, timeoutMsg: `Poster still displayed at ${w}px viewport.` },
      );
    }
  });
});
