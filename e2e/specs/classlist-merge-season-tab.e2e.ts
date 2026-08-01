import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  NowPlayingState,
  VideoHome,
  VideoLibraryItem,
  VideoSeasonEpisodes,
  VideoShowDetail,
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

const episode: VideoLibraryItem = {
  id: 'e2e-ep-1',
  name: 'Pilot',
  itemType: 'Episode',
  overview: null,
  productionYear: 2024,
  runtimeSeconds: 2700,
  played: false,
  favorite: false,
  artworkImageId: null,
  seasonNumber: 1,
  episodeNumber: 1,
  seriesId: 'e2e-show',
  seriesName: 'E2E Merge Show',
  resumePositionSeconds: null,
  playedPercentage: null,
};

const showDetail = {
  id: 'e2e-show',
  name: 'E2E Merge Show',
  overview: 'A show used to prove season tab merge semantics.',
  productionYear: 2024,
  genres: ['Drama'],
  played: false,
  favorite: false,
  canPlay: false,
  artworkImageId: null,
  backdropImageId: null,
  nextEpisode: null,
  seasons: [
    {
      id: 'e2e-season-1',
      name: 'Season 1',
      seasonNumber: 1,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
    {
      id: 'e2e-season-2',
      name: 'Season 2',
      seasonNumber: 2,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
    {
      id: 'e2e-season-3',
      name: 'Season 3',
      seasonNumber: 3,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
  ],
  metadata: {
    communityRating: null,
    officialRating: null,
    creators: [],
    cast: [],
  },
} as const satisfies VideoShowDetail;

const seasonEpisodes = {
  seriesId: 'e2e-show',
  seasonId: 'e2e-season-1',
  seasonNumber: 1,
  episodes: [episode],
} as const satisfies VideoSeasonEpisodes;

const emptyVideoHome = {
  continueWatching: [],
  nextUp: [],
  latestMovies: [],
  latestEpisodes: [],
} as const satisfies VideoHome;

const userDataUpdate = {
  itemId: 'e2e-show',
  played: false,
  favorite: true,
} as const satisfies VideoUserDataUpdate;

const fixtures = {
  server_is_connected: true,
  server_get_state: connectedState,
  library_video_home: emptyVideoHome,
  library_video_shortcuts: [] as const,
  library_show_detail: showDetail,
  library_season_episodes: seasonEpisodes,
  library_similar_video: [] as const,
  library_update_user_data: userDataUpdate,
  library_play: null,
  now_playing_get_state: offlineNowPlaying,
} as const;

// Primary token #4f46e5; the pre-fix bug let the transparent base win.
const PRIMARY_BG = 'rgb(79, 70, 229)';
const TRANSPARENT_BG = 'rgba(0, 0, 0, 0)';

function tabBackground(pressed: boolean): Promise<string> {
  return browser.execute((wanted) => {
    const tab = document.querySelector(
      `ul[aria-label="Show seasons"] button[aria-pressed="${wanted}"]`,
    );
    if (!tab) return `missing-tab-${wanted}`;
    return getComputedStyle(tab).backgroundColor;
  }, pressed);
}

describe('Season tab active merge semantics', () => {
  it('keeps the primary background on the active tab, including on hover and after switching', async () => {
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
      controller.installFixture('library_show_detail', {
        kind: 'return',
        value: values.library_show_detail,
      });
      controller.installFixture('library_season_episodes', {
        kind: 'return',
        value: values.library_season_episodes,
      });
      controller.installFixture('library_similar_video', {
        kind: 'return',
        value: [...values.library_similar_video],
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

    await browser.execute(() => {
      window.history.pushState({}, '', '/library/shows/e2e-show');
      window.dispatchEvent(new PopStateEvent('popstate'));
    });

    const tabs = await $('ul[aria-label="Show seasons"]');
    await tabs.waitForDisplayed({ timeout: 30_000 });

    // Initial state: season 1 active (primary), others transparent.
    await browser.waitUntil(async () => (await tabBackground(true)) === PRIMARY_BG, {
      timeout: 10_000,
      timeoutMsg: 'Active season tab did not render the primary background.',
    });
    expect(await tabBackground(false)).toBe(TRANSPARENT_BG);

    // The WebKitGTK driver cannot synthesize :hover, so prove the nested
    // _pressed._hover rule at the CSSOM level: the generated stylesheet must
    // carry a pressed+hover rule that re-declares the primary background,
    // which is what keeps the active tab primary under the pointer.
    const hoverRule = await browser.execute(() => {
      const matches: { selector: string; background: string }[] = [];
      const visit = (rules: CSSRuleList): void => {
        for (const rule of rules) {
          if (rule instanceof CSSStyleRule) {
            if (
              rule.selectorText.includes('aria-pressed') &&
              rule.selectorText.includes(':hover')
            ) {
              matches.push({
                selector: rule.selectorText,
                background: rule.style.background || rule.style.backgroundColor,
              });
            }
          } else if (rule instanceof CSSGroupingRule) {
            visit(rule.cssRules);
          }
        }
      };
      for (const sheet of document.styleSheets) {
        visit(sheet.cssRules);
      }
      return matches;
    });
    const pressedHover = hoverRule.find((rule) => rule.background === 'var(--colors-primary)');
    expect(pressedHover).toBeDefined();

    // Switching tabs moves the pressed state and the primary background.
    const seasonTwo = await $('ul[aria-label="Show seasons"] li:nth-child(2) button');
    await seasonTwo.click();
    await browser.waitUntil(
      () =>
        browser.execute(() => {
          const tab = document.querySelector(
            'ul[aria-label="Show seasons"] li:nth-child(2) button',
          );
          return (
            tab?.getAttribute('aria-pressed') === 'true' &&
            getComputedStyle(tab).backgroundColor === 'rgb(79, 70, 229)'
          );
        }),
      {
        timeout: 10_000,
        timeoutMsg: 'Clicked season tab did not become active with the primary background.',
      },
    );
    await browser.waitUntil(async () => (await tabBackground(false)) === TRANSPARENT_BG, {
      timeout: 5000,
      timeoutMsg: 'Previously active season tab did not revert to the transparent base.',
    });
  });
});
