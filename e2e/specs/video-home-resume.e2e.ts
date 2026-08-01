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
      overview: null,
    },
  ],
  nextUp: [],
  latestMovies: [
    {
      id: 'e2e-latest-movie',
      name: 'E2E Latest Movie',
      itemType: 'Movie',
      seriesId: null,
      seriesName: null,
      seasonNumber: null,
      episodeNumber: null,
      productionYear: 2025,
      runtimeSeconds: null,
      resumePositionSeconds: null,
      playedPercentage: null,
      played: false,
      favorite: false,
      artworkImageId: null,
      overview: null,
    },
  ],
  latestEpisodes: [],
} as const satisfies VideoHome;

const itemDetail = {
  id: 'e2e-home-movie',
  name: 'E2E Home Movie',
  itemType: 'Movie',
  overview: 'E2E Home Movie overview.',
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
  metadata: { communityRating: null, officialRating: null, creators: [], cast: [] },
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
  library_similar_video: [],
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
      controller.installFixture('library_similar_video', {
        kind: 'return',
        value: [...values.library_similar_video],
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

    // The resume-first hero fills from the matching item detail fixture.
    const featuredHeading = await $('aria/E2E Home Movie');
    await featuredHeading.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(
      () =>
        browser.execute(
          () => document.body.textContent?.includes('E2E Home Movie overview.') === true,
        ),
      {
        timeout: 30_000,
        timeoutMsg: 'The featured hero overview did not render from the item detail fixture.',
      },
    );

    const resume = await $('aria/Resume featured E2E Home Movie');
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

    const heroDetails = await $('aria/Details');
    await heroDetails.click();
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => window.location.pathname)) === '/library/items/e2e-home-movie',
      {
        timeout: 30_000,
        timeoutMsg: 'Hero Details did not navigate to the item detail route.',
      },
    );

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

    const backToHome = await $('aria/Back');
    await backToHome.click();
    await browser.waitUntil(
      async () => (await browser.execute(() => window.location.pathname)) === '/library',
      { timeout: 5000, timeoutMsg: 'Detail back navigation did not return to the library.' },
    );

    // Reference density: row track counts follow the measured row width
    // through the tuned landscape/poster ladders (4 landscape + 6 poster at
    // the isolated 1600×900 default, 3 + 5 at 1280×720).
    const expectedTracks = (aspect: 'video' | 'poster', width: number): number => {
      const ladder: readonly (readonly [number, number])[] =
        aspect === 'video'
          ? [
              [560, 2],
              [820, 3],
              [1120, 4],
              [1380, 5],
            ]
          : [
              [560, 3],
              [700, 4],
              [950, 5],
              [1160, 6],
              [1390, 7],
            ];
      let expected = aspect === 'video' ? 1 : 2;
      for (const [minimum, count] of ladder) {
        if (width >= minimum) {
          expected = count;
        }
      }
      return expected;
    };
    const measureRow = (rowId: string): Promise<{ tracks: number; width: number } | null> =>
      browser.execute((id: string) => {
        const section = document.querySelector(`section[aria-labelledby="row-${id}"]`);
        const grid = section?.querySelector<HTMLElement>(':scope > div:nth-of-type(2)');
        if (!grid) return null;
        return {
          tracks: getComputedStyle(grid).gridTemplateColumns.split(' ').length,
          width: grid.clientWidth,
        };
      }, rowId);
    const continueSection = await $('section[aria-labelledby="row-continue-watching"]');
    await continueSection.waitForDisplayed({ timeout: 30_000 });

    const findOverflowOffenders = (): Promise<string[]> =>
      browser.execute(() => {
        const offenders: string[] = [];
        for (const element of document.querySelectorAll<HTMLElement>('body *')) {
          const rect = element.getBoundingClientRect();
          if (rect.right > window.innerWidth + 1 || rect.left < -1) {
            offenders.push(
              `${element.tagName} ${element.className.toString().slice(0, 60)} left=${Math.round(rect.left)} right=${Math.round(rect.right)}`,
            );
          }
        }
        return offenders.slice(0, 8);
      });

    const defaultContinue = await measureRow('continue-watching');
    const defaultMovies = await measureRow('latest-movies');
    expect(defaultContinue).not.toBeNull();
    expect(defaultMovies).not.toBeNull();
    expect(defaultContinue!.tracks).toBe(expectedTracks('video', defaultContinue!.width));
    expect(defaultMovies!.tracks).toBe(expectedTracks('poster', defaultMovies!.width));
    expect(await findOverflowOffenders()).toEqual([]);

    await browser.setWindowSize(1280, 720);
    await browser.waitUntil(
      async () => {
        const measured = await measureRow('continue-watching');
        return measured !== null && measured.tracks === expectedTracks('video', measured.width);
      },
      {
        timeout: 5000,
        timeoutMsg: 'Continue Watching tracks did not follow the ladder at 1280×720.',
      },
    );
    const resizedContinue = (await measureRow('continue-watching'))!;
    const resizedMovies = (await measureRow('latest-movies'))!;
    expect(resizedMovies.tracks).toBe(expectedTracks('poster', resizedMovies.width));
    expect(resizedContinue.tracks).toBeLessThanOrEqual(defaultContinue!.tracks);
    expect(resizedMovies.tracks).toBeLessThanOrEqual(defaultMovies!.tracks);
    await browser.waitUntil(
      async () => {
        const offenders = await findOverflowOffenders();
        return offenders.length === 0;
      },
      {
        timeout: 5000,
        timeoutMsg: 'Home content overflowed horizontally at 1280×720.',
      },
    );
  });
});
