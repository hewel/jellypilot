import { $, browser, expect } from '@wdio/globals';

import type {
  ConnectionState,
  NowPlayingState,
  VideoHome,
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
  itemCount: 240,
  artworkImageId: null,
} as const satisfies VideoLibraryShortcut;

const browseItems: VideoLibraryItem[] = Array.from({ length: 24 }, (_, index) => ({
  id: `virtual-e2e-movie-${index + 1}`,
  name: `Virtual E2E Movie ${index + 1}`,
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
  overview: null,
}));

const browsePage = {
  collectionType: 'movies',
  libraryId: 'movies',
  startIndex: 0,
  limit: 24,
  totalRecordCount: 240,
  hasMore: true,
  items: browseItems,
} as const satisfies VideoLibraryPage;

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
  library_video_home: videoHome,
  library_video_shortcuts: [moviesShortcut],
  library_browse_video: browsePage,
  library_item_shortcut: moviesShortcut,
  now_playing_get_state: offlineState,
} as const;

describe('library virtual scrolling', () => {
  it('keeps a rendered row across large native scroll jumps', async () => {
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
    await sidebar.$('aria/Movies').click();

    const firstCard = await $('aria/Open Virtual E2E Movie 1');
    await firstCard.waitForDisplayed({ timeout: 30_000 });

    const result = await browser.execute(async () => {
      const viewport = document.querySelector<HTMLElement>(
        '[role="region"][aria-label="Application content"]',
      );
      const virtualGrid = document.querySelector<HTMLElement>(
        '[role="grid"][aria-label="Movies library items"]',
      );
      if (!viewport || !virtualGrid) {
        throw new Error('The Movies virtual grid was not rendered.');
      }

      const totalHeight = Number(virtualGrid.style.height.replace('px', ''));
      const lastOffset = Math.max(totalHeight - viewport.clientHeight, 0);
      const offsets = [0, totalHeight * 0.4, totalHeight * 0.75, totalHeight * 0.2, lastOffset];
      const results: {
        offset: number;
        renderedRows: number;
        intersectsViewport: boolean;
      }[] = [];
      const blankPhases: string[] = [];
      const sample = (phase: string) => {
        const viewportRect = viewport.getBoundingClientRect();
        const rows = [...virtualGrid.children];
        const intersectsViewport = rows.some((row) => {
          const rect = row.getBoundingClientRect();
          return rect.bottom > viewportRect.top && rect.top < viewportRect.bottom;
        });
        if (rows.length === 0 || !intersectsViewport) {
          blankPhases.push(phase);
        }
        return {
          renderedRows: rows.length,
          intersectsViewport,
        };
      };
      const observer = new MutationObserver(() => sample('mutation'));
      observer.observe(virtualGrid, { childList: true });

      for (const offset of offsets) {
        viewport.scrollTop = offset;
        viewport.dispatchEvent(new Event('scroll'));
        await Promise.resolve();
        sample('microtask');
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        sample('first-animation-frame');
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        const settled = sample('second-animation-frame');
        results.push({
          offset,
          ...settled,
        });
      }

      observer.disconnect();
      return { blankPhases, samples: results };
    });

    expect(result.blankPhases).toEqual([]);
    expect(result.samples.every((sample) => sample.renderedRows > 0)).toBe(true);
    expect(result.samples.every((sample) => sample.intersectsViewport)).toBe(true);
  });
});
