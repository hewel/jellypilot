// @rstest-environment jsdom
import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import type { QueryClient } from '@tanstack/solid-query';
import { RouterProvider, createMemoryHistory } from '@tanstack/solid-router';
import { fireEvent, screen, waitFor, within } from '@testing-library/dom';
import { Exit } from 'effect';
import { render } from 'solid-js/web';

import { commands, events } from '../src/bindings';
import type {
  AppConfig,
  CommandError,
  NowPlayingState,
  VideoHome,
  VideoItemDetail,
  VideoItemStreams,
  VideoLibraryItem,
  VideoLibraryPage,
  VideoLibraryShortcut,
  VideoSearchPage,
  VideoSeasonEpisodes,
  VideoShowDetail,
} from '../src/bindings';
import { itemThumb } from '../src/components/AppSidebar.styles';
import { ToastProvider } from '../src/components/ToastProvider';
import { queryKeys } from '../src/effects/query';
import { createJellyPilotRouter } from '../src/router';
import * as browseRouteStyles from '../src/routes/_authenticated/library/browseRoute.styles';
import { resetSharedLibraryFilters } from '../src/utils/createSharedLibraryFilters';
import { imageSource, resetImageProxyBase } from '../src/utils/imageSource';
import { resetSidebarPreferences } from '../src/utils/sidebarPreferences';
import { resetSidebarWipe } from '../src/utils/sidebarWipe';
import { createTestQueryClient, TestQueryProvider } from './query-client';

type LibraryItemDetailResult =
  | { status: 'ok'; data: VideoItemDetail }
  | { status: 'error'; error: CommandError };
type LibraryItemStreamsResult =
  | { status: 'ok'; data: VideoItemStreams }
  | { status: 'error'; error: CommandError };

interface TestIntersectionObserverController {
  trigger(isIntersecting?: boolean): void;
}

interface TestTauriStoreController {
  reset(): void;
  get(path: string, key: string): unknown;
  getCount(path: string, key: string): number;
  loadCount(path: string): number;
  set(path: string, key: string, value: unknown): void;
}

declare global {
  interface Window {
    __TEST_INTERSECTION_OBSERVER__: TestIntersectionObserverController;
    __TEST_TAURI_STORE__: TestTauriStoreController;
  }
}

// Mock scrollTo since JSDOM doesn't implement layout/scrolling APIs
Element.prototype.scrollTo = function (this: Element, arg?: ScrollToOptions | number, y?: number) {
  if (typeof arg === 'number') {
    this.scrollLeft = arg;
    this.scrollTop = y ?? 0;
    return;
  }

  if (arg?.left != null) {
    this.scrollLeft = arg.left;
  }
  if (arg?.top != null) {
    this.scrollTop = arg.top;
  }
};
window.scrollTo = () => {};

const connectedState = {
  capabilities: {
    introSkipper: true,
    quickConnect: true,
    remoteControl: true,
    remoteControlAvailable: true,
    remoteControlWarning: null,
  },
  connected: true,
  provider: 'jellyfin' as const,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

const disconnectedState = {
  ...connectedState,
  connected: false,
};

const secondConnectedState = {
  ...connectedState,
  serverName: 'Second Server',
  serverUrl: 'https://second.example.com',
  userId: 'user-2',
  userName: 'Grace',
};

const nowPlaying: NowPlayingState = {
  canPlayNext: true,
  canPlayPrevious: false,
  media: {
    episodeNumber: 1,
    itemId: 'episode-1',
    itemType: 'Episode',
    name: 'The Pilot',
    seasonNumber: 1,
    seriesName: 'Example Show',
  },
  nextUnavailableReason: null,
  player: {
    connected: true,
    duration: 180,
    muted: false,
    paused: false,
    timePos: 42,
    volume: 80,
  },
  previousUnavailableReason: 'noCurrentItem',
  status: 'playing',
};

const nowPlayingTrackList = JSON.stringify([
  { id: 1, selected: true, title: 'English Stereo', type: 'audio' },
  { id: 2, selected: false, title: 'Japanese 5.1', type: 'audio' },
  { id: 3, selected: true, title: 'English Subtitles', type: 'sub' },
]);

const config: AppConfig = {
  deviceName: 'JellyPilot Test',
  introSkipperMode: 'automatic',
  keybindIntroSkip: 'g',
  keybindNext: 'Shift+>',
  keybindPrev: 'Shift+<',
  mpvArgs: [],
  mpvPath: null,
  preferredSubtitleLanguages: [],
  progressInterval: 5,
  startMinimized: false,
};

const shellCleanups: (() => void)[] = [];

const audioStreams = [
  {
    codec: 'aac',
    index: 1,
    isDefault: true,
    isExternal: false,
    label: 'English - AAC 2.0',
    language: 'eng',
  },
  {
    codec: 'flac',
    index: 2,
    isDefault: false,
    isExternal: false,
    label: 'Japanese - FLAC 5.1',
    language: 'jpn',
  },
];

const subtitleStreams = [
  {
    codec: 'srt',
    index: 3,
    isDefault: false,
    isExternal: true,
    label: 'English - SRT',
    language: 'eng',
  },
];

const videoHome: VideoHome = {
  continueWatching: [
    {
      id: 'movie-1',
      name: 'Resume Movie',
      itemType: 'Movie',
      seriesId: null,
      seriesName: null,
      seasonNumber: null,
      episodeNumber: null,
      productionYear: 2024,
      runtimeSeconds: 7200,
      resumePositionSeconds: 120,
      playedPercentage: 25,
      played: true,
      favorite: true,
      artworkImageId: 'https://jellyfin.example.com/Items/movie-1/Images/Primary',
    },
  ],
  latestEpisodes: [
    {
      id: 'episode-2',
      name: 'Latest Episode',
      itemType: 'Episode',
      seriesId: 'series-1',
      seriesName: 'Example Show',
      seasonNumber: 1,
      episodeNumber: 3,
      productionYear: null,
      runtimeSeconds: null,
      resumePositionSeconds: null,
      playedPercentage: null,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
  ],
  latestMovies: [
    {
      id: 'movie-2',
      name: 'Latest Movie',
      itemType: 'Movie',
      seriesId: null,
      seriesName: null,
      seasonNumber: null,
      episodeNumber: null,
      productionYear: null,
      runtimeSeconds: null,
      resumePositionSeconds: null,
      playedPercentage: null,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
  ],
  nextUp: [
    {
      id: 'episode-1',
      name: 'Next Episode',
      itemType: 'Episode',
      seriesId: 'series-1',
      seriesName: 'Example Show',
      seasonNumber: 1,
      episodeNumber: 2,
      productionYear: null,
      runtimeSeconds: 1800,
      resumePositionSeconds: null,
      playedPercentage: null,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
  ],
};

const videoLibraryShortcuts: VideoLibraryShortcut[] = [
  {
    id: 'movies',
    name: 'Movies',
    collectionType: 'movies',
    itemCount: 8,
    artworkImageId: null,
  },
  {
    id: 'shows',
    name: 'Shows',
    collectionType: 'tvshows',
    itemCount: 5,
    artworkImageId: null,
  },
];

const movieDetail: VideoItemDetail = {
  artworkImageId: 'https://jellyfin.example.com/Items/detail-movie/Images/Primary',
  backdropImageId: 'https://jellyfin.example.com/Items/detail-movie/Images/Backdrop/0',
  canPlay: true,
  canResume: true,
  episodeNumber: null,
  favorite: true,
  genres: ['Drama', 'Mystery'],
  id: 'detail-movie',
  itemType: 'Movie',
  name: 'Detail Movie',
  overview: 'A movie overview.',
  played: false,
  playedPercentage: 25,
  productionYear: 2024,
  resumePositionSeconds: 120,
  runtimeSeconds: 7200,
  seasonNumber: null,
  seriesId: null,
  seriesName: null,
};

const episodeDetail: VideoItemDetail = {
  artworkImageId: null,
  backdropImageId: null,
  canPlay: true,
  canResume: false,
  episodeNumber: 3,
  favorite: false,
  genres: ['Sci-Fi'],
  id: 'detail-episode',
  itemType: 'Episode',
  name: 'Detail Episode',
  overview: null,
  played: true,
  playedPercentage: 100,
  productionYear: null,
  resumePositionSeconds: 0,
  runtimeSeconds: null,
  seasonNumber: 2,
  seriesId: 'series-1',
  seriesName: 'Example Show',
  subtitleStreams,
};

const itemStreams: VideoItemStreams = {
  audioStreams,
  subtitleStreams,
};

const nextEpisodeDetail: VideoItemDetail = {
  ...episodeDetail,
  canResume: false,
  episodeNumber: 2,
  id: 'episode-2',
  name: 'Next Episode',
  played: false,
  playedPercentage: null,
  resumePositionSeconds: null,
  seasonNumber: 1,
};

const showDetail: VideoShowDetail = {
  artworkImageId: null,
  backdropImageId: null,
  canPlay: true,
  favorite: false,
  genres: ['Drama'],
  id: 'series-1',
  name: 'Example Show',
  nextEpisode: {
    artworkImageId: null,
    episodeNumber: 2,
    favorite: false,
    id: 'episode-2',
    itemType: 'Episode',
    name: 'Next Episode',
    played: false,
    playedPercentage: null,
    productionYear: null,
    resumePositionSeconds: null,
    runtimeSeconds: null,
    seasonNumber: 1,
    seriesId: 'series-1',
    seriesName: 'Example Show',
  },
  overview: 'A show overview.',
  played: false,
  productionYear: 2023,
  seasons: [
    {
      id: 'season-1',
      name: 'Season 1',
      seasonNumber: 1,
      played: false,
      favorite: false,
      artworkImageId: null,
    },
    {
      id: 'season-2',
      name: 'Season 2',
      seasonNumber: 2,
      played: false,
      favorite: true,
      artworkImageId: null,
    },
  ],
};

const seasonEpisodes: VideoSeasonEpisodes = {
  episodes: [
    {
      id: 'episode-2',
      name: 'Next Episode',
      itemType: 'Episode',
      productionYear: null,
      runtimeSeconds: 1800,
      played: false,
      favorite: false,
      artworkImageId: null,
      seasonNumber: 1,
      episodeNumber: 2,
      seriesId: 'series-1',
      seriesName: 'Example Show',
      resumePositionSeconds: null,
      playedPercentage: null,
    },
  ],
  seasonId: 'season-1',
  seasonNumber: 1,
  seriesId: 'series-1',
};

function videoLibraryPage(startIndex: number): VideoLibraryPage {
  if (startIndex === 0) {
    return {
      collectionType: 'movies',
      hasMore: true,
      items: [
        {
          id: 'movie-1',
          name: 'Paged Movie',
          itemType: 'Movie',
          productionYear: 2025,
          runtimeSeconds: 5400,
          played: false,
          favorite: true,
          artworkImageId: 'https://jellyfin.example.com/Items/movie-1/Images/Primary',
          seasonNumber: null,
          episodeNumber: null,
          seriesId: null,
          seriesName: null,
          resumePositionSeconds: null,
          playedPercentage: null,
        },
      ],
      libraryId: 'movies',
      limit: 24,
      startIndex: 0,
      totalRecordCount: 25,
    };
  }

  return {
    collectionType: 'movies',
    hasMore: false,
    items: [
      {
        id: 'movie-25',
        name: 'Paged Movie 25',
        itemType: 'Movie',
        productionYear: null,
        runtimeSeconds: null,
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
    ],
    libraryId: 'movies',
    limit: 24,
    startIndex,
    totalRecordCount: 25,
  };
}

function largeVideoLibraryPage(startIndex: number): VideoLibraryPage {
  const endIndex = Math.min(startIndex + 24, 125);

  return {
    collectionType: 'movies',
    hasMore: startIndex + 24 < 125,
    items: Array.from({ length: endIndex - startIndex }, (_, offset) => {
      const index = startIndex + offset;

      return {
        id: `virtual-movie-${index + 1}`,
        name: `Virtual Movie ${index + 1}`,
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
      };
    }),
    libraryId: 'movies',
    limit: 24,
    startIndex,
    totalRecordCount: 125,
  };
}

const searchResultPool: VideoLibraryItem[] = [
  {
    id: 'alien-movie',
    name: 'Alien',
    itemType: 'Movie',
    productionYear: 1979,
    runtimeSeconds: 7020,
    played: true,
    favorite: true,
    artworkImageId: 'alien-movie-art',
    seasonNumber: null,
    episodeNumber: null,
    seriesId: null,
    seriesName: null,
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  {
    id: 'alien-episode',
    name: 'Alien Covenant: Homecoming',
    itemType: 'Episode',
    productionYear: 2025,
    runtimeSeconds: 3000,
    played: true,
    favorite: false,
    artworkImageId: null,
    seasonNumber: 1,
    episodeNumber: 2,
    seriesId: 'alien-show',
    seriesName: 'Alien Earth',
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  {
    id: 'alien-show',
    name: 'Alien Earth',
    itemType: 'Series',
    productionYear: 2025,
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
  ...Array.from({ length: 21 }, (_, index) => ({
    id: `alien-extra-${index + 1}`,
    name: `Alien Extra ${String(index + 1).padStart(2, '0')}`,
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
    id: 'alien-rematch',
    name: 'Alien Rematch',
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
  },
];

function videoSearchPage(startIndex: number): VideoSearchPage {
  return {
    query: 'alien',
    startIndex,
    limit: 24,
    totalRecordCount: searchResultPool.length,
    hasMore: startIndex + 24 < searchResultPool.length,
    items: searchResultPool.slice(startIndex, startIndex + 24),
  };
}

function mockShellCommands(state = connectedState) {
  rstest.spyOn(commands, 'appLocalServices').mockResolvedValue({
    imageProxyBase: 'http://127.0.0.1:43127',
  });
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(true);
  rstest.spyOn(commands, 'serverGetState').mockResolvedValue(state);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: {
      activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
      profiles: [
        {
          active: true,
          key: 'jellyfin|https://jellyfin.example.com|Ada',
          lastRestoreError: null,
          reauthRequired: false,
          provider: 'jellyfin',
          serverName: 'Jellyfin Home',
          serverUrl: 'https://jellyfin.example.com',
          userName: 'Ada',
        },
      ],
    },
    status: 'ok',
  });
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue(config);
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    data: videoHome,
    status: 'ok',
  });
  rstest.spyOn(commands, 'libraryVideoShortcuts').mockResolvedValue({
    data: videoLibraryShortcuts,
    status: 'ok',
  });
  rstest.spyOn(commands, 'libraryItemShortcut').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      data: videoLibraryPage(request.startIndex),
      status: 'ok',
    }),
  );
  rstest.spyOn(commands, 'librarySearchVideo').mockImplementation((request) =>
    Promise.resolve({
      data: videoSearchPage(request.startIndex),
      status: 'ok',
    }),
  );
  rstest.spyOn(commands, 'libraryItemDetail').mockImplementation((itemId) => {
    const data =
      itemId === 'detail-episode'
        ? episodeDetail
        : itemId === 'episode-2'
          ? nextEpisodeDetail
          : movieDetail;

    return Promise.resolve({ data, status: 'ok' });
  });
  rstest.spyOn(commands, 'libraryItemStreams').mockResolvedValue({
    data: itemStreams,
    status: 'ok',
  });
  rstest.spyOn(commands, 'libraryShowDetail').mockResolvedValue({
    data: showDetail,
    status: 'ok',
  });
  rstest.spyOn(commands, 'librarySeasonEpisodes').mockResolvedValue({
    data: seasonEpisodes,
    status: 'ok',
  });
  rstest.spyOn(commands, 'libraryPlay').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'libraryUpdateUserData').mockResolvedValue({
    data: { favorite: false, itemId: 'detail-movie', played: false },
    status: 'ok',
  });
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    data: nowPlaying,
    status: 'ok',
  });
  rstest.spyOn(commands, 'mpvGetProperty').mockResolvedValue({
    data: nowPlayingTrackList,
    status: 'ok',
  });
  rstest.spyOn(events.nowPlayingChanged, 'listen').mockResolvedValue(() => {});
}

function appScrollViewport(): HTMLElement {
  const viewport = screen.queryByRole('region', { name: 'Application content' });
  if (viewport) {
    return viewport;
  }

  throw new Error('App scroll viewport was not rendered');
}

function scrollVirtualGridToEnd(viewport: HTMLElement) {
  const canvas = screen.getByTestId('library-virtual-grid').firstElementChild as HTMLElement | null;
  if (!canvas) {
    throw new Error('Library virtual canvas was not rendered');
  }

  const totalHeight = Number(canvas.style.height.replace('px', ''));
  viewport.scrollTop = Math.max(totalHeight - window.innerHeight, 0);
  fireEvent.scroll(viewport);
}

function observeVirtualGridBlanking(virtualGrid: HTMLElement) {
  const canvas = virtualGrid.firstElementChild as HTMLElement | null;
  if (!canvas) {
    throw new Error('Library virtual canvas was not rendered');
  }

  let observedBlankWindow = canvas.childElementCount === 0;
  const sample = () => {
    observedBlankWindow ||= canvas.childElementCount === 0;
  };
  const observer = new MutationObserver(sample);
  observer.observe(canvas, { childList: true });

  return {
    sample,
    stop: () => {
      observer.disconnect();
      sample();
      return observedBlankWindow;
    },
  };
}

function renderShell(path: string | string[] = '/library', client?: QueryClient) {
  const root = document.createElement('div');
  document.body.append(root);
  const initialEntries = Array.isArray(path) ? path : [path];
  const router = createJellyPilotRouter(
    createMemoryHistory({
      initialEntries,
      initialIndex: initialEntries.length - 1,
    }),
  );
  const dispose = render(
    () => (
      <TestQueryProvider client={client}>
        <ToastProvider>
          <RouterProvider router={router} />
        </ToastProvider>
      </TestQueryProvider>
    ),
    root,
  );

  const cleanup = () => {
    const cleanupIndex = shellCleanups.indexOf(cleanup);
    if (cleanupIndex !== -1) {
      shellCleanups.splice(cleanupIndex, 1);
    }
    dispose();
    root.remove();
  };
  shellCleanups.push(cleanup);
  return cleanup;
}

beforeEach(() => {
  resetSharedLibraryFilters();
  resetSidebarPreferences();
  resetSidebarWipe();
  resetImageProxyBase();
  window.__TEST_TAURI_STORE__.reset();
});

afterEach(() => {
  while (shellCleanups.length > 0) {
    shellCleanups.pop()?.();
  }
  rstest.restoreAllMocks();
  resetSharedLibraryFilters();
  resetSidebarPreferences();
  resetSidebarWipe();
  resetImageProxyBase();
  document.body.innerHTML = '';
  localStorage.clear();
  sessionStorage.clear();
  window.__TEST_TAURI_STORE__.reset();
});

test('authenticated shell renders the persistent Sidebar and drops floating controls', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Sidebar' });

  // No shell header: no app-area navigation, brand, or user/server badge.
  expect(screen.queryByRole('navigation', { name: 'JellyPilot areas' })).toBeNull();
  expect(screen.queryByRole('link', { name: 'Settings' })).toBeNull();
  expect(screen.queryByRole('link', { name: 'Diagnostics' })).toBeNull();
  expect(screen.queryByText('Control Room')).toBeNull();
  expect(screen.queryByText(connectedState.userName)).toBeNull();
  expect(screen.queryByText(connectedState.serverName)).toBeNull();

  // The floating cluster is gone; Now Playing and Open Settings live in the Sidebar.
  expect(screen.queryByRole('group', { name: 'Floating controls' })).toBeNull();
  const sidebar = screen.getByRole('navigation', { name: 'Sidebar' });
  await waitFor(() =>
    expect(
      within(sidebar).getByRole('button', { name: /Now Playing: Playing — The Pilot/ }),
    ).toBeVisible(),
  );
  expect(within(sidebar).getByRole('button', { name: 'Open Settings' })).toBeVisible();

  expect(document.querySelector('[data-scope="scroll-area"][data-part="root"]')).toBeNull();
  expect(appScrollViewport()).toBeVisible();
  expect(screen.getByRole('main')).toBeVisible();

  cleanup();
});

test('Sidebar collapse toggle collapses the rail and persists the preference', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library');

  await screen.findByRole('navigation', { name: 'Sidebar' });

  const toggle = await screen.findByRole('button', { name: 'Collapse sidebar' });
  expect(toggle).toHaveAttribute('aria-expanded', 'true');
  // The toggle sits at the top of the sidebar, before the navigation links.
  const firstNavLink = screen.getByRole('link', { name: 'Home' });
  expect(
    toggle.compareDocumentPosition(firstNavLink) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();

  fireEvent.click(toggle);

  const expandToggle = await screen.findByRole('button', { name: 'Expand sidebar' });
  expect(expandToggle).toHaveAttribute('aria-expanded', 'false');
  await waitFor(() =>
    expect(window.__TEST_TAURI_STORE__.get('preferences.json', 'sidebar_collapsed')).toBe(true),
  );

  fireEvent.click(expandToggle);

  const restored = await screen.findByRole('button', { name: 'Collapse sidebar' });
  expect(restored).toHaveAttribute('aria-expanded', 'true');
  await waitFor(() =>
    expect(window.__TEST_TAURI_STORE__.get('preferences.json', 'sidebar_collapsed')).toBe(false),
  );

  cleanup();
});

test('Sidebar collapse plays the wipe overlay and clears it after the animation', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library');

  const toggle = await screen.findByRole('button', { name: 'Collapse sidebar' });
  expect(screen.queryByTestId('sidebar-wipe')).toBeNull();

  fireEvent.click(toggle);

  expect(await screen.findByTestId('sidebar-wipe')).toBeVisible();
  expect(screen.getByRole('navigation', { name: 'Sidebar' })).toHaveAttribute(
    'data-wiping',
    'true',
  );

  await waitFor(() => expect(screen.queryByTestId('sidebar-wipe')).toBeNull());
  expect(screen.getByRole('navigation', { name: 'Sidebar' })).not.toHaveAttribute('data-wiping');

  cleanup();
});

test('Sidebar restores the collapsed preference from the Tauri Store', async () => {
  mockShellCommands();
  window.__TEST_TAURI_STORE__.set('preferences.json', 'sidebar_collapsed', true);

  const cleanup = renderShell('/library');

  await screen.findByRole('navigation', { name: 'Sidebar' });

  const toggle = await screen.findByRole('button', { name: 'Expand sidebar' });
  expect(toggle).toHaveAttribute('aria-expanded', 'false');

  cleanup();
});

test('library landing renders command-backed rows and drawer trigger', async () => {
  mockShellCommands();
  const localServices = rstest.mocked(commands.appLocalServices);
  const playCommand = rstest.spyOn(commands, 'libraryPlay');
  const cleanup = renderShell();
  await screen.findByRole('navigation', { name: 'Sidebar' });

  const navigation = screen.getByRole('navigation', { name: 'Sidebar' });
  expect(navigation).toBeVisible();
  expect(screen.getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page');
  expect(screen.getByRole('link', { name: 'Movies' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Shows' })).toBeVisible();
  expect(await screen.findByRole('heading', { name: 'Continue Watching' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Resume Resume Movie' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Open Next Episode' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Open Latest Movie' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Open Latest Episode' })).toBeVisible();
  const resumeMovieButton = screen.getByRole('button', { name: 'Resume Resume Movie' });
  expect(resumeMovieButton).toBeVisible();
  expect(within(resumeMovieButton).queryByRole('img', { name: 'Played' })).toBeNull();
  expect(within(resumeMovieButton).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '25');
  // Title/subtitle sit outside the action control so hover popups do not steal resume clicks.
  const remainingLabel = screen.getByText('118 mins remaining');
  expect(remainingLabel).toBeVisible();
  expect(resumeMovieButton.contains(remainingLabel)).toBe(false);
  const resumeArtwork = screen.getByAltText('Resume Movie artwork');
  await waitFor(() =>
    expect(resumeArtwork).toHaveAttribute(
      'src',
      'http://127.0.0.1:43127/image/https%3A%2F%2Fjellyfin.example.com%2FItems%2Fmovie-1%2FImages%2FPrimary',
    ),
  );
  expect(localServices).toHaveBeenCalledTimes(1);
  expect(resumeArtwork.parentElement).toHaveAttribute('data-aspect', 'video');
  fireEvent.load(resumeArtwork);
  expect(resumeArtwork.parentElement).toHaveAttribute('data-aspect', 'video');
  const latestMovieLink = screen.getByRole('link', { name: 'Open Latest Movie' });
  const latestMovieTitle = screen.getByText('Latest Movie • Movie');
  expect(latestMovieTitle).toBeVisible();
  expect(latestMovieLink.contains(latestMovieTitle)).toBe(false);
  expect(screen.queryByText('Movie · null')).toBeNull();
  expect(latestMovieLink.querySelector('[data-aspect="poster"]')).not.toBeNull();
  expect(screen.getAllByText('No artwork')).toHaveLength(3);
  const latestEpisodeLink = screen.getByRole('link', { name: 'Open Latest Episode' });
  expect(latestEpisodeLink.querySelector('svg')).not.toBeNull();
  fireEvent.click(resumeMovieButton);
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      audioStreamIndex: null,
      itemId: 'movie-1',
      mode: 'resume',
      startPositionSeconds: 120,
      subtitleStreamIndex: null,
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole('button', { name: /Now Playing: Playing — The Pilot/ })).toBeVisible(),
  );

  cleanup();
});

test('library landing expands rows independently from measured capacity', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    data: {
      ...videoHome,
      continueWatching: Array.from({ length: 4 }, (_, index) => ({
        ...videoHome.continueWatching[0]!,
        id: `resume-${index + 1}`,
        name: `Resume ${index + 1}`,
      })),
      nextUp: Array.from({ length: 4 }, (_, index) => ({
        ...videoHome.nextUp[0]!,
        id: `next-${index + 1}`,
        name: `Next ${index + 1}`,
      })),
    },
    status: 'ok',
  });
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Continue Watching' });
  await screen.findByRole('button', { name: /Now Playing: Playing — The Pilot/ });
  const continueHeading = screen.getByRole('heading', { name: 'Continue Watching' });
  const continueSection = continueHeading.closest('section');
  const nextSection = screen.getByRole('heading', { name: 'Next Up' }).closest('section');
  expect(continueSection).not.toBeNull();
  expect(nextSection).not.toBeNull();
  expect(within(continueSection!).getAllByRole('button', { name: /^Resume Resume/ })).toHaveLength(
    3,
  );
  expect(within(nextSection!).getAllByRole('link', { name: /^Open Next/ })).toHaveLength(3);

  const continueDisclosure = within(continueSection!).getByRole('button', { name: 'See All' });
  expect(continueDisclosure).toHaveAttribute('aria-expanded', 'false');
  const controlledGridId = continueDisclosure.getAttribute('aria-controls') ?? '';
  expect(document.querySelector(`[id="${controlledGridId}"]`)).not.toBeNull();

  fireEvent.click(continueDisclosure);
  const expandedContinueHeading = await screen.findByRole('heading', {
    name: 'Continue Watching',
  });
  const expandedContinueSection = expandedContinueHeading.closest('section');
  expect(
    within(expandedContinueSection!).getAllByRole('button', { name: /^Resume Resume/ }),
  ).toHaveLength(4);
  expect(
    within(expandedContinueSection!).getByRole('button', { name: 'Show Less' }),
  ).toHaveAttribute('aria-expanded', 'true');
  expect(
    within(screen.getByRole('heading', { name: 'Next Up' }).closest('section')!).getByRole(
      'button',
      { name: 'See All' },
    ),
  ).toHaveAttribute('aria-expanded', 'false');
  expect(
    within(screen.getByRole('heading', { name: 'Next Up' }).closest('section')!).getAllByRole(
      'link',
      { name: /^Open Next/ },
    ),
  ).toHaveLength(3);

  fireEvent.click(
    within(expandedContinueSection!).getByRole('button', {
      name: 'Show Less',
    }),
  );
  await waitFor(() =>
    expect(
      within(
        screen.getByRole('heading', { name: 'Continue Watching' }).closest('section')!,
      ).getByRole('button', { name: 'See All' }),
    ).toHaveAttribute('aria-expanded', 'false'),
  );
  await waitFor(() =>
    expect(
      within(
        screen.getByRole('heading', { name: 'Continue Watching' }).closest('section')!,
      ).getAllByRole('button', { name: /^Resume Resume/ }),
    ).toHaveLength(3),
  );

  cleanup();
});

test('library landing blocks concurrent resume requests and shows selected-card progress', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    data: {
      ...videoHome,
      continueWatching: [
        videoHome.continueWatching[0]!,
        {
          ...videoHome.continueWatching[0]!,
          id: 'movie-2-resume',
          name: 'Second Resume',
        },
      ],
    },
    status: 'ok',
  });
  let finishPlayback!: (value: { data: null; status: 'ok' }) => void;
  const playCommand = rstest.spyOn(commands, 'libraryPlay').mockImplementation(
    () =>
      new Promise((resolve) => {
        finishPlayback = resolve;
      }),
  );
  const cleanup = renderShell();

  const first = await screen.findByRole('button', { name: 'Resume Resume Movie' });
  const second = screen.getByRole('button', { name: 'Resume Second Resume' });
  fireEvent.click(first);

  expect(await screen.findByRole('button', { name: 'Starting Resume Movie' })).toBeDisabled();
  expect(second).toBeDisabled();
  await waitFor(() => expect(playCommand).toHaveBeenCalledTimes(1));
  fireEvent.click(second);
  expect(playCommand).toHaveBeenCalledTimes(1);

  finishPlayback({ data: null, status: 'ok' });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Resume Resume Movie' })).not.toBeDisabled(),
  );

  cleanup();
});

test('library landing reports direct resume failures through the existing toast', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryPlay').mockResolvedValue({
    error: { code: 'network', message: 'Resume failed' },
    status: 'error',
  });
  const cleanup = renderShell();

  fireEvent.click(await screen.findByRole('button', { name: 'Resume Resume Movie' }));

  expect(await screen.findByText('Resume failed')).toBeVisible();
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Resume Resume Movie' })).not.toBeDisabled(),
  );

  cleanup();
});

test('library browse auto-loads paged results and opens detail links without playback', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell('/library/movies/movies');

  const navigation = await screen.findByRole('navigation', { name: 'Sidebar' });
  expect(navigation).toBeVisible();
  expect(screen.getByRole('link', { name: 'Movies' })).toHaveAttribute('aria-current', 'page');
  expect(screen.getByRole('link', { name: 'Home' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Shows' })).toBeVisible();
  const controls = await screen.findByRole('navigation', { name: 'Library browse controls' });
  expect(within(controls).getByRole('button', { name: 'Sort By' })).toBeVisible();
  expect(within(controls).getByRole('button', { name: 'Status' })).toBeVisible();
  expect(within(controls).getByRole('button', { name: 'Sort ascending' })).toBeVisible();
  expect(within(controls).getByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(within(controls).getByText(/\d+ of \d+/)).toBeVisible();
  const pagedMovieLink = await screen.findByRole('link', {
    name: 'Open Paged Movie, favorite',
  });
  expect(pagedMovieLink).toHaveAttribute('href', '/library/items/movie-1');
  expect(screen.queryByText('Favorite')).toBeNull();
  expect(within(pagedMovieLink).queryByRole('img', { name: 'Unplayed' })).toBeNull();
  expect(within(pagedMovieLink).queryByText('Unplayed')).toBeNull();
  expect(screen.getByAltText('Paged Movie artwork')).toBeVisible();
  expect(browseCommand).toHaveBeenCalledWith({
    collectionType: 'movies',
    favoritesOnly: false,
    libraryId: 'movies',
    limit: 24,
    playedFilter: 'all',
    sort: 'title',
    startIndex: 0,
  });

  pagedMovieLink.addEventListener('click', (event) => event.preventDefault());
  fireEvent.click(pagedMovieLink);
  expect(mpvStart).not.toHaveBeenCalled();

  expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();
  window.__TEST_INTERSECTION_OBSERVER__.trigger(true);
  const pagedMovie25Link = await screen.findByRole('link', { name: /Paged Movie 25/ });
  expect(pagedMovie25Link).toHaveAttribute('href', '/library/items/movie-25');
  expect(within(pagedMovie25Link).getByRole('img', { name: 'Played' })).toBeVisible();
  expect(browseCommand).toHaveBeenLastCalledWith({
    collectionType: 'movies',
    favoritesOnly: false,
    libraryId: 'movies',
    limit: 24,
    playedFilter: 'all',
    sort: 'title',
    startIndex: 24,
  });
  expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();

  cleanup();
});
test('detail page highlights parent library in Sidebar', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryItemShortcut').mockResolvedValue({
    data: {
      id: 'movies',
      name: 'Movies',
      collectionType: 'movies',
      itemCount: 1,
      artworkImageId: null,
    },
    status: 'ok',
  });
  const cleanup = renderShell('/library/items/detail-movie');

  await screen.findByRole('heading', { name: 'Detail Movie' });
  await waitFor(() =>
    expect(screen.getByRole('link', { name: 'Movies' })).toHaveAttribute('aria-current', 'page'),
  );
  expect(screen.getByRole('link', { name: 'Home' })).not.toHaveAttribute('aria-current');

  cleanup();
});

test('sidebar shows library artwork with icon fallback', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryVideoShortcuts').mockResolvedValue({
    data: [
      { ...videoLibraryShortcuts[0]!, artworkImageId: 'movies-art' },
      videoLibraryShortcuts[1]!,
    ],
    status: 'ok',
  });
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Sidebar' });
  const moviesLink = await screen.findByRole('link', { name: 'Movies' });
  const moviesArtwork = moviesLink.querySelector('img');
  expect(moviesArtwork).toHaveAttribute('src', imageSource('movies-art'));
  // Contractual fixed 24px thumb slot: without it the artwork renders at natural
  // size and pushes the row labels out of the sidebar.
  expect(moviesArtwork).toHaveClass(itemThumb);
  const showsLink = screen.getByRole('link', { name: 'Shows' });
  expect(showsLink.querySelector('img')).toBeNull();
  expect(showsLink.querySelector('svg')).not.toBeNull();

  cleanup();
});

test('library browse redirects home when active server changes under stale library URL', async () => {
  mockShellCommands();
  const queryClient = createTestQueryClient();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies', queryClient);

  expect(await screen.findByRole('link', { name: /Paged Movie/ })).toBeVisible();
  browseCommand.mockClear();

  queryClient.setQueryData(queryKeys.connectionState, Exit.succeed(secondConnectedState));

  expect(await screen.findByRole('heading', { name: 'Continue Watching' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page');
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(browseCommand).not.toHaveBeenCalled();

  cleanup();
});

test('library browse retries failed auto-loaded page', async () => {
  mockShellCommands();
  let nextPageShouldFail = true;
  rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) => {
    if (request.startIndex === 24 && nextPageShouldFail) {
      return Promise.resolve({
        error: { code: 'internal', message: 'Next page failed' },
        status: 'error',
      });
    }

    return Promise.resolve({
      data: videoLibraryPage(request.startIndex),
      status: 'ok',
    });
  });
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('link', { name: /Paged Movie/ });
  window.__TEST_INTERSECTION_OBSERVER__.trigger(true);

  expect(await screen.findByText('Next page failed')).toBeVisible();
  const retryButton = screen.getByRole('button', { name: 'Retry loading more' });
  expect(retryButton).toBeVisible();

  nextPageShouldFail = false;
  fireEvent.click(retryButton);

  expect(await screen.findByRole('link', { name: /Paged Movie 25/ })).toBeVisible();
  await waitFor(() => expect(screen.queryByText('Next page failed')).toBeNull());

  cleanup();
});

test('library browse virtualizes large libraries and fetches visible placeholder pages', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      data: largeVideoLibraryPage(request.startIndex),
      status: 'ok',
    }),
  );
  const cleanup = renderShell('/library/movies/movies');

  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  const virtualGrid = screen.getByTestId('library-virtual-grid');
  expect(virtualGrid).toBeVisible();
  // A persistent entrance animation restarts when virtual rows churn, fading
  // the whole grid back to transparent during scroll and pointer transitions.
  expect(virtualGrid).not.toHaveClass(browseRouteStyles.fade);
  expect(screen.getByRole('grid', { name: 'Movies library items' })).toBe(
    virtualGrid.firstElementChild,
  );
  expect(screen.queryByRole('link', { name: 'Open Virtual Movie 125' })).toBeNull();
  expect(screen.getAllByRole('link', { name: /Open Virtual Movie/ }).length).toBeLessThan(125);

  const viewport = appScrollViewport();
  const virtualCanvas = virtualGrid.firstElementChild;
  expect(virtualCanvas).not.toBeNull();
  await new Promise((resolve) => setTimeout(resolve, 0));

  const totalVirtualHeight = Number((virtualCanvas as HTMLElement).style.height.replace('px', ''));
  expect(totalVirtualHeight).toBeGreaterThan(window.innerHeight);
  const lastViewportOffset = Math.max(totalVirtualHeight - window.innerHeight, 0);
  const blankingObserver = observeVirtualGridBlanking(virtualGrid);
  for (const scrollTop of [
    totalVirtualHeight * 0.4,
    totalVirtualHeight * 0.75,
    totalVirtualHeight * 0.2,
    lastViewportOffset,
  ]) {
    viewport.scrollTop = scrollTop;
    fireEvent.scroll(viewport);

    blankingObserver.sample();
    await Promise.resolve();
    blankingObserver.sample();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    blankingObserver.sample();
  }
  expect(blankingObserver.stop()).toBe(false);

  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'all',
      sort: 'title',
      startIndex: 120,
    }),
  );
  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 125' })).toBeVisible();

  cleanup();
});

test('library browse reuses cached virtual pages on route re-entry', async () => {
  mockShellCommands();
  const queryClient = createTestQueryClient();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      data: largeVideoLibraryPage(request.startIndex),
      status: 'ok',
    }),
  );
  const firstCleanup = renderShell('/library/movies/movies', queryClient);

  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  const firstViewport = appScrollViewport();
  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollVirtualGridToEnd(firstViewport);
  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 125' })).toBeVisible();

  firstViewport.scrollTop = 0;
  fireEvent.scroll(firstViewport);
  firstCleanup();
  browseCommand.mockClear();
  const secondCleanup = renderShell('/library/movies/movies', queryClient);

  // Re-query inside waitFor: cached re-entry remounts virtual rows after measure,
  // so a one-shot findByRole node can detach before toBeVisible runs.
  await waitFor(() => {
    expect(screen.getByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  });
  const secondViewport = appScrollViewport();
  await new Promise((resolve) => setTimeout(resolve, 0));
  scrollVirtualGridToEnd(secondViewport);
  await waitFor(() => {
    expect(screen.getByRole('link', { name: 'Open Virtual Movie 125' })).toBeVisible();
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(browseCommand).toHaveBeenCalledTimes(1);
  expect(browseCommand).toHaveBeenCalledWith({
    collectionType: 'movies',
    favoritesOnly: false,
    libraryId: 'movies',
    limit: 24,
    playedFilter: 'all',
    sort: 'title',
    startIndex: 0,
  });

  secondCleanup();
});

test('library browse reuses cached switched library pages before virtual pages', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      data: {
        ...largeVideoLibraryPage(request.startIndex),
        collectionType: request.collectionType,
        libraryId: request.libraryId,
      },
      status: 'ok',
    }),
  );
  const cleanup = renderShell('/library/movies/movies');

  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  fireEvent.click(screen.getByRole('link', { name: 'Shows' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'tvshows',
      favoritesOnly: false,
      libraryId: 'shows',
      limit: 24,
      playedFilter: 'all',
      sort: 'title',
      startIndex: 0,
    }),
  );

  const viewport = appScrollViewport();
  scrollVirtualGridToEnd(viewport);
  browseCommand.mockClear();

  fireEvent.click(screen.getByRole('link', { name: 'Movies' }));
  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  await new Promise((resolve) => setTimeout(resolve, 0));

  const movieRequests = browseCommand.mock.calls
    .map(([request]) => request)
    .filter((request) => request.collectionType === 'movies' && request.libraryId === 'movies');
  expect(movieRequests).toHaveLength(1);
  expect(movieRequests[0]?.startIndex).toBe(0);

  cleanup();
});

test('library browse resets scroll to top when the sort direction changes', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      data: largeVideoLibraryPage(request.startIndex),
      status: 'ok',
    }),
  );
  const cleanup = renderShell('/library/movies/movies');

  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  const viewport = appScrollViewport();
  scrollVirtualGridToEnd(viewport);
  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 125' })).toBeVisible();

  const scrollToSpy = rstest.spyOn(Element.prototype, 'scrollTo');
  browseCommand.mockClear();
  fireEvent.click(screen.getByRole('button', { name: 'Sort ascending' }));

  await waitFor(() => expect(scrollToSpy).toHaveBeenCalledWith({ top: 0 }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'all',
      sort: 'title',
      startIndex: 0,
    }),
  );

  scrollToSpy.mockRestore();
  cleanup();
});

test('library browse retries failed virtual placeholder page', async () => {
  mockShellCommands();
  let bottomPageShouldFail = true;
  rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) => {
    if (request.startIndex === 120 && bottomPageShouldFail) {
      return Promise.resolve({
        error: { code: 'internal', message: 'Virtual page failed' },
        status: 'error',
      });
    }

    return Promise.resolve({
      data: largeVideoLibraryPage(request.startIndex),
      status: 'ok',
    });
  });
  const cleanup = renderShell('/library/movies/movies');

  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 1' })).toBeVisible();
  const viewport = appScrollViewport();
  scrollVirtualGridToEnd(viewport);

  expect(await screen.findByText('Virtual page failed')).toBeVisible();
  const retryButton = screen.getByRole('button', { name: 'Retry loading more' });
  await waitFor(() => expect(retryButton).not.toBeDisabled());

  bottomPageShouldFail = false;
  fireEvent.click(retryButton);

  expect(await screen.findByRole('link', { name: 'Open Virtual Movie 125' })).toBeVisible();
  await waitFor(() => expect(screen.queryByText('Virtual page failed')).toBeNull());

  cleanup();
});

test('library browse controls reload paged results from the first page', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('link', { name: /Paged Movie/ });
  fireEvent.click(screen.getByRole('button', { name: 'Sort By' }));
  fireEvent.click(screen.getByText('Recently added', { selector: 'span' }));

  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'all',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );

  await waitFor(() => expect(screen.getByRole('button', { name: 'Status' })).not.toBeDisabled());
  fireEvent.click(screen.getByRole('button', { name: 'Status' }));
  fireEvent.click(screen.getByText('Unplayed', { selector: 'span' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'unplayed',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );

  await waitFor(() => expect(screen.getByRole('button', { name: /Status/ })).not.toBeDisabled());
  fireEvent.click(screen.getByRole('button', { name: /Status/ }));
  fireEvent.click(screen.getByText('Favorites Only', { selector: 'span' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: true,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'unplayed',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Sort ascending' })).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Sort ascending' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: true,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'unplayed',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );

  cleanup();
});

test('library browse controls are shared across libraries', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('link', { name: /Paged Movie/ });
  fireEvent.click(screen.getByRole('button', { name: 'Sort By' }));
  fireEvent.click(screen.getByText('Recently added', { selector: 'span' }));
  fireEvent.click(screen.getByRole('button', { name: 'Status' }));
  fireEvent.click(screen.getByText('Unplayed', { selector: 'span' }));
  fireEvent.click(screen.getByRole('button', { name: /Status/ }));
  fireEvent.click(screen.getByText('Favorites Only', { selector: 'span' }));
  fireEvent.click(screen.getByRole('button', { name: 'Sort ascending' }));

  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Sort descending' })).toBeVisible(),
  );
  await waitFor(() =>
    expect(window.__TEST_TAURI_STORE__.get('preferences.json', 'library_filters')).toEqual({
      sort: 'recentlyAdded',
      playedFilter: 'unplayed',
      favoritesOnly: true,
      sortDirection: 'desc',
    }),
  );
  fireEvent.click(screen.getByRole('link', { name: 'Shows' }));

  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'tvshows',
      favoritesOnly: true,
      libraryId: 'shows',
      limit: 24,
      playedFilter: 'unplayed',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );
  expect(screen.getByRole('button', { name: 'Sort descending' })).toBeVisible();

  cleanup();
});

test('library browse hydrates shared filters once across route remounts', async () => {
  mockShellCommands();
  window.__TEST_TAURI_STORE__.set('preferences.json', 'library_filters', {
    sort: 'recentlyAdded',
    playedFilter: 'all',
    favoritesOnly: false,
    sortDirection: 'asc',
  });
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/tvshows/shows');

  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'tvshows',
      favoritesOnly: false,
      libraryId: 'shows',
      limit: 24,
      playedFilter: 'all',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );
  // Filters and the sidebar preference both hydrate from the same store on mount.
  // Wait for both before asserting load counts stay stable across remounts.
  await waitFor(() =>
    expect(window.__TEST_TAURI_STORE__.getCount('preferences.json', 'sidebar_collapsed')).toBe(1),
  );
  const loadsAfterFirstHydration = window.__TEST_TAURI_STORE__.loadCount('preferences.json');
  expect(loadsAfterFirstHydration).toBeGreaterThanOrEqual(1);
  expect(window.__TEST_TAURI_STORE__.getCount('preferences.json', 'library_filters')).toBe(1);

  fireEvent.click(screen.getByRole('link', { name: 'Movies' }));

  await waitFor(() =>
    expect(browseCommand).toHaveBeenCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'all',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );
  expect(window.__TEST_TAURI_STORE__.loadCount('preferences.json')).toBe(loadsAfterFirstHydration);
  expect(window.__TEST_TAURI_STORE__.getCount('preferences.json', 'library_filters')).toBe(1);

  cleanup();
});

test('library browse hydrates filters from migrated Tauri Store preferences', async () => {
  mockShellCommands();
  window.__TEST_TAURI_STORE__.set('preferences.json', 'library_filters', {
    sort: 'releaseDate',
    playedFilter: 'played',
    favoritesOnly: true,
    sortDirection: 'desc',
  });
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies');

  const expectedRequest = {
    collectionType: 'movies',
    favoritesOnly: true,
    libraryId: 'movies',
    limit: 24,
    playedFilter: 'played',
    sort: 'releaseDate',
    startIndex: 0,
  };
  await waitFor(() => expect(browseCommand).toHaveBeenCalledWith(expectedRequest));
  expect(browseCommand.mock.calls[0]?.[0]).toEqual(expectedRequest);

  cleanup();
});

test('library browse migrates legacy localStorage filters into Tauri Store', async () => {
  mockShellCommands();
  localStorage.setItem('jellypilot_library_filters', 'recentlyAdded|unplayed|1|desc');
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies');

  const expectedRequest = {
    collectionType: 'movies',
    favoritesOnly: true,
    libraryId: 'movies',
    limit: 24,
    playedFilter: 'unplayed',
    sort: 'recentlyAdded',
    startIndex: 0,
  };
  await waitFor(() => expect(browseCommand).toHaveBeenCalledWith(expectedRequest));
  expect(browseCommand.mock.calls[0]?.[0]).toEqual(expectedRequest);
  await waitFor(() =>
    expect(window.__TEST_TAURI_STORE__.get('preferences.json', 'library_filters')).toEqual({
      sort: 'recentlyAdded',
      playedFilter: 'unplayed',
      favoritesOnly: true,
      sortDirection: 'desc',
    }),
  );
  expect(localStorage.getItem('jellypilot_library_filters')).toBeNull();

  cleanup();
});

test('library browse surfaces backend sort and filter errors', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryBrowseVideo').mockResolvedValue({
    error: { code: 'internal', message: 'Unsupported library filter' },
    status: 'error',
  });
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByText('Unsupported library filter');
  expect(screen.queryByRole('link', { name: /Paged Movie/ })).toBeNull();

  cleanup();
});

test('library item detail renders resume-primary movie metadata', async () => {
  mockShellCommands();
  const playCommand = rstest.spyOn(commands, 'libraryPlay');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell('/library/items/detail-movie');

  await screen.findByRole('heading', { name: 'Detail Movie' });
  expect(screen.getByRole('button', { name: 'Back' })).toBeVisible();
  expect(screen.getByText('A movie overview.')).toBeVisible();
  expect(screen.getByText('Drama')).toBeVisible();
  expect(screen.getByText('Mystery')).toBeVisible();
  expect(screen.getByText('Favorite')).toBeVisible();
  expect(screen.getByText('2h 0m')).toBeVisible();
  expect(await screen.findByAltText('Detail Movie backdrop')).toHaveAttribute(
    'src',
    imageSource(movieDetail.backdropImageId ?? ''),
  );
  expect(screen.getByRole('button', { name: 'Resume' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Play from beginning' })).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      audioStreamIndex: null,
      itemId: 'detail-movie',
      mode: 'resume',
      startPositionSeconds: 120,
      subtitleStreamIndex: null,
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Play from beginning' })).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Play from beginning' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      audioStreamIndex: null,
      itemId: 'detail-movie',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: null,
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Play from beginning' })).not.toBeDisabled(),
  );
  expect(mpvStart).not.toHaveBeenCalled();

  cleanup();
});

test('library item detail back returns to the previous library route when history exists', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library/movies/movies');

  const pagedMovieLink = await screen.findByRole('link', { name: 'Open Paged Movie, favorite' });
  const viewport = appScrollViewport();
  viewport.scrollTop = 432;
  fireEvent.scroll(viewport);

  fireEvent.click(pagedMovieLink);

  expect(await screen.findByRole('heading', { name: 'Detail Movie' })).toBeVisible();
  await waitFor(() => expect(viewport.scrollTop).toBe(0));

  fireEvent.click(screen.getByRole('button', { name: 'Back' }));

  expect(await screen.findByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(await screen.findByRole('link', { name: 'Open Paged Movie, favorite' })).toBeVisible();
  await waitFor(() => expect(screen.queryByRole('heading', { name: 'Detail Movie' })).toBeNull());
  await waitFor(() => expect(viewport.scrollTop).toBe(432));

  cleanup();
});

test('library item detail back returns home when opened from the library landing', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library');

  const latestMovieLink = await screen.findByRole('link', { name: 'Open Latest Movie' });
  fireEvent.click(latestMovieLink);

  expect(await screen.findByRole('heading', { name: 'Detail Movie' })).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Back' }));

  expect(await screen.findByRole('heading', { name: 'Continue Watching' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page');
  await waitFor(() => expect(screen.queryByRole('heading', { name: 'Detail Movie' })).toBeNull());

  cleanup();
});

test('library item detail leaves the skeleton after an asynchronous detail response', async () => {
  mockShellCommands();
  let resolveDetail!: (result: LibraryItemDetailResult) => void;
  const detailCommand = rstest.spyOn(commands, 'libraryItemDetail').mockImplementation(
    () =>
      new Promise<LibraryItemDetailResult>((resolve) => {
        resolveDetail = resolve;
      }),
  );
  const { promise: streamsPromise, resolve: resolveStreams } =
    Promise.withResolvers<LibraryItemStreamsResult>();
  const streamsCommand = rstest
    .spyOn(commands, 'libraryItemStreams')
    .mockImplementation(() => streamsPromise);
  const cleanup = renderShell('/library');

  const latestMovieLink = await screen.findByRole('link', { name: 'Open Latest Movie' });
  fireEvent.click(latestMovieLink);

  await waitFor(() => expect(detailCommand).toHaveBeenCalledWith('movie-2'));
  expect(screen.queryByRole('heading', { name: 'Detail Movie' })).toBeNull();
  expect(screen.getByRole('status', { name: 'Loading item detail' })).toBeVisible();

  resolveDetail({ data: movieDetail, status: 'ok' });

  expect(await screen.findByRole('heading', { name: 'Detail Movie' })).toBeVisible();
  expect(screen.getByText('A movie overview.')).toBeVisible();
  expect(detailCommand).toHaveBeenCalledTimes(1);
  await waitFor(() => expect(streamsCommand).toHaveBeenCalledWith('movie-2'));
  expect(screen.queryByText('eng, jpn')).toBeNull();

  resolveStreams({ data: itemStreams, status: 'ok' });
  expect(await screen.findByText('eng, jpn')).toBeVisible();

  cleanup();
});

test('library item detail refreshes user data only after mutation success', async () => {
  mockShellCommands();
  const updateCommand = rstest.spyOn(commands, 'libraryUpdateUserData');
  rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValueOnce({ data: movieDetail, status: 'ok' })
    .mockResolvedValueOnce({
      data: { ...movieDetail, favorite: false },
      status: 'ok',
    });
  const cleanup = renderShell('/library/items/detail-movie');

  await screen.findByRole('heading', { name: 'Detail Movie' });
  fireEvent.click(screen.getByRole('button', { name: 'Unfavorite' }));

  await waitFor(() =>
    expect(updateCommand).toHaveBeenCalledWith({
      action: 'unfavorite',
      itemId: 'detail-movie',
    }),
  );
  expect(await screen.findByText('Not favorite')).toBeVisible();

  cleanup();
});

test('library item detail keeps previous user data visible on mutation failure', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryUpdateUserData').mockResolvedValue({
    error: { code: 'network', message: 'Favorite update failed' },
    status: 'error',
  });
  const cleanup = renderShell('/library/items/detail-movie');

  await screen.findByRole('heading', { name: 'Detail Movie' });
  fireEvent.click(screen.getByRole('button', { name: 'Unfavorite' }));

  expect(await screen.findByText('Favorite update failed')).toBeVisible();
  expect(screen.getByText('Favorite')).toBeVisible();
  expect(screen.queryByText('Not favorite')).toBeNull();

  cleanup();
});

test('library item detail renders episode metadata and semantic artwork placeholder', async () => {
  mockShellCommands();
  const playCommand = rstest.spyOn(commands, 'libraryPlay');
  const cleanup = renderShell('/library/items/detail-episode');

  await screen.findByRole('heading', { name: 'Detail Episode' });
  expect(screen.getByRole('link', { name: 'Example Show' })).toHaveAttribute(
    'href',
    '/library/shows/series-1',
  );
  expect(screen.getByText(/S02E03/)).toBeVisible();
  expect(screen.getByText('Played')).toBeVisible();
  expect(screen.getByText('Not favorite')).toBeVisible();
  expect(screen.getByText('Sci-Fi')).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Resume' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Play' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      audioStreamIndex: null,
      itemId: 'detail-episode',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: null,
    }),
  );

  cleanup();
});

test('library show detail auto-loads next-up season and renders episode rows', async () => {
  mockShellCommands();
  const showCommand = rstest.spyOn(commands, 'libraryShowDetail');
  const itemCommand = rstest.spyOn(commands, 'libraryItemDetail');
  const seasonCommand = rstest.spyOn(commands, 'librarySeasonEpisodes');
  const playCommand = rstest.spyOn(commands, 'libraryPlay');
  const updateCommand = rstest.spyOn(commands, 'libraryUpdateUserData');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell('/library/shows/series-1');

  await screen.findByRole('heading', { name: 'Example Show' });
  expect(screen.getByRole('button', { name: 'Back' })).toBeVisible();
  expect(screen.getByText('A show overview.')).toBeVisible();
  expect(screen.getByText('Drama')).toBeVisible();
  expect(screen.getByText('Unplayed')).toBeVisible();
  expect(screen.getByText('Not favorite')).toBeVisible();

  // Series user data controls
  fireEvent.click(screen.getByRole('button', { name: 'Favorite' }));
  await waitFor(() =>
    expect(updateCommand).toHaveBeenCalledWith({
      action: 'favorite',
      itemId: 'series-1',
    }),
  );

  fireEvent.click(screen.getByRole('button', { name: 'Play S01E02' }));
  await waitFor(() => expect(itemCommand).toHaveBeenCalledWith('episode-2'));
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      audioStreamIndex: null,
      itemId: 'episode-2',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: null,
    }),
  );

  // Season selector buttons
  expect(screen.getByRole('button', { name: 'Season 1' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Season 2' })).toBeVisible();
  expect(showCommand).toHaveBeenCalledWith('series-1');

  // Auto-load: nextEpisode.seasonNumber=1, so season 1 loads automatically
  await waitFor(() =>
    expect(seasonCommand).toHaveBeenCalledWith({
      seasonId: 'season-1',
      seasonNumber: 1,
      seriesId: 'series-1',
    }),
  );

  // Wait for episodes to render with dense rows
  await waitFor(() => {
    expect(screen.getByText('S01E02')).toBeVisible();
    expect(screen.getByRole('link', { name: 'Next Episode' })).toHaveAttribute(
      'href',
      '/library/items/episode-2',
    );
    expect(screen.getByText('30m')).toBeVisible();
  });

  // Inline episode play button
  const episodePlayBtn = screen.getByRole('button', { name: 'Play' });
  expect(episodePlayBtn).toBeVisible();
  fireEvent.click(episodePlayBtn);
  await waitFor(() => expect(itemCommand).toHaveBeenLastCalledWith('episode-2'));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      audioStreamIndex: null,
      itemId: 'episode-2',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: null,
    }),
  );

  // Manual season switch
  fireEvent.click(screen.getByRole('button', { name: 'Season 2' }));
  await waitFor(() =>
    expect(seasonCommand).toHaveBeenLastCalledWith({
      seasonId: 'season-2',
      seasonNumber: 2,
      seriesId: 'series-1',
    }),
  );

  // Show episode link back navigation
  const episodeLink = await screen.findByRole('link', { name: 'Next Episode' });
  fireEvent.click(episodeLink);
  expect(await screen.findByRole('heading', { name: 'Next Episode' })).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Back' }));
  expect(await screen.findByRole('heading', { name: 'Example Show' })).toBeVisible();
  await waitFor(() => expect(screen.queryByRole('heading', { name: 'Next Episode' })).toBeNull());

  expect(mpvStart).not.toHaveBeenCalled();

  cleanup();
});

test('library landing has no retry and skips video home when disconnected', async () => {
  mockShellCommands(disconnectedState);
  const videoHomeCommand = rstest.spyOn(commands, 'libraryVideoHome');
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Sidebar' });
  expect(screen.queryByRole('button', { name: 'Retry Library' })).toBeNull();
  expect(videoHomeCommand).not.toHaveBeenCalled();

  cleanup();
});

test('library landing renders no fake content on command error', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(true);
  rstest.spyOn(commands, 'serverGetState').mockResolvedValue(connectedState);
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    error: { code: 'network', message: 'Jellyfin unavailable' },
    status: 'error',
  });
  rstest.spyOn(commands, 'libraryVideoShortcuts').mockResolvedValue({
    data: [],
    status: 'ok',
  });
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    data: nowPlaying,
    status: 'ok',
  });
  rstest.spyOn(events.nowPlayingChanged, 'listen').mockResolvedValue(() => {});
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Sidebar' });
  expect(screen.queryByRole('button', { name: 'Retry Library' })).toBeNull();
  expect(screen.queryByText('Continue Watching')).toBeNull();

  cleanup();
});

test('library landing renders no rows for empty video home', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    data: {
      continueWatching: [],
      latestEpisodes: [],
      latestMovies: [],
      nextUp: [],
    },
    status: 'ok',
  });
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Sidebar' });
  expect(screen.queryByRole('button', { name: 'Retry Library' })).toBeNull();
  expect(screen.queryByText('No artwork')).toBeNull();

  cleanup();
});

test('now playing drawer exposes full playback controls', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Sidebar' });

  const trigger = await screen.findByRole('button', { name: /Now Playing: Playing — The Pilot/ });
  fireEvent.click(trigger);

  const dialog = await screen.findByRole('dialog', { name: 'Now Playing' });
  expect(dialog).toBeVisible();
  expect(await screen.findByText('The Pilot')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Pause' })).toBeVisible();
  expect(await screen.findByRole('slider', { name: 'Seek position' })).toBeVisible();

  const setAudioTrack = rstest
    .spyOn(commands, 'mpvSetAudioTrack')
    .mockResolvedValue({ data: null, status: 'ok' });
  await waitFor(() => expect(screen.getAllByText('English Stereo').length).toBeGreaterThan(0));
  fireEvent.click(screen.getAllByText('English Stereo')[0]?.closest('button') as HTMLButtonElement);
  fireEvent.click(await screen.findByRole('option', { name: 'Japanese 5.1' }));
  await waitFor(() => expect(setAudioTrack).toHaveBeenCalledWith(2));

  expect(screen.getByRole('dialog', { name: 'Now Playing' })).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Close Now Playing' }));
  await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Now Playing' })).toBeNull());

  cleanup();
});

test('Settings modal opens operations console content and closes via Close button and Escape', async () => {
  mockShellCommands();
  localStorage.setItem(
    'jellypilot_auth_session',
    JSON.stringify({ serverUrl: 'https://jellypilot.example' }),
  );

  const cleanup = renderShell('/library');
  await screen.findByRole('navigation', { name: 'Sidebar' });

  const trigger = await screen.findByRole('button', { name: 'Open Settings' });
  expect(trigger).toBeVisible();
  fireEvent.click(trigger);

  const settings = await screen.findByRole('dialog', { name: 'Settings' });
  expect(settings).toBeVisible();
  expect(
    within(settings).getByText(
      'Connection, player bridge, diagnostics, shortcuts, and session controls',
    ),
  ).toBeVisible();
  expect(within(settings).getByRole('heading', { name: 'Connection' })).toBeVisible();
  expect(within(settings).getByRole('heading', { name: 'Player Bridge settings' })).toBeVisible();
  expect(within(settings).getByRole('heading', { name: 'Diagnostics' })).toBeVisible();
  expect(within(settings).getByText('0 sanitized runtime events')).toBeVisible();

  expect(within(settings).getByRole('button', { name: 'Disconnect' })).toBeVisible();
  expect(
    within(settings).getByText(
      'Disconnect ends the active media server connection but keeps saved services available for Reconnect.',
    ),
  ).toBeVisible();
  expect(within(settings).getByRole('button', { name: 'Sign out' })).toBeVisible();
  expect(
    within(settings).getByText(
      'Sign out removes the active saved service and leaves any other saved services available.',
    ),
  ).toBeVisible();
  expect(localStorage.getItem('jellypilot_auth_session')).not.toBeNull();

  fireEvent.click(screen.getByRole('button', { name: 'Close Settings' }));
  await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull());
  expect(screen.getByRole('navigation', { name: 'Sidebar' })).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
  const reopened = await screen.findByRole('dialog', { name: 'Settings' });
  reopened.focus();
  fireEvent.keyDown(reopened, { code: 'Escape', key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull());
  expect(screen.getByRole('navigation', { name: 'Sidebar' })).toBeVisible();

  cleanup();
});

test('shell search bar stays disabled while disconnected', async () => {
  mockShellCommands(disconnectedState);
  const cleanup = renderShell('/library/movies/movies');

  const input = await screen.findByRole('searchbox', { name: 'Search library' });
  expect(input).toBeDisabled();
  expect(input).toHaveAttribute('placeholder', 'Connect to search');
  expect(screen.getByRole('button', { name: 'Search library' })).toBeDisabled();

  fireEvent.keyDown(document.body, { key: '/' });
  expect(input).not.toHaveFocus();

  cleanup();
});

test('shell search slash shortcut focuses retained draft outside inputs and stays inert while an input is focused', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('heading', { name: 'Movies' });
  const input = (await screen.findByRole('searchbox', {
    name: 'Search library',
  })) as HTMLInputElement;

  input.focus();
  const notPrevented = fireEvent.keyDown(input, { key: '/' });
  expect(notPrevented).toBe(true);
  expect(input).toHaveValue('');

  fireEvent.input(input, { target: { value: 'alien' } });
  expect(commands.librarySearchVideo).not.toHaveBeenCalled();

  input.blur();
  expect(input).not.toHaveFocus();

  const prevented = fireEvent.keyDown(document.body, { key: '/' });
  expect(prevented).toBe(false);
  expect(input).toHaveFocus();
  expect(input).toHaveValue('alien');
  expect(input.selectionStart).toBe(0);
  expect(input.selectionEnd).toBe('alien'.length);
  expect(commands.librarySearchVideo).not.toHaveBeenCalled();

  cleanup();
});

test('shell search draft enablement, whitespace rejection, clear, and trimming submit flow', async () => {
  mockShellCommands();
  const searchCommand = rstest.spyOn(commands, 'librarySearchVideo');
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('heading', { name: 'Movies' });
  const input = (await screen.findByRole('searchbox', {
    name: 'Search library',
  })) as HTMLInputElement;
  const submit = screen.getByRole('button', { name: 'Search library' });
  const form = input.closest('form') as HTMLFormElement;

  expect(submit).toBeDisabled();

  fireEvent.input(input, { target: { value: '   ' } });
  expect(submit).toBeDisabled();
  fireEvent.submit(form);
  expect(searchCommand).not.toHaveBeenCalled();

  fireEvent.input(input, { target: { value: 'alien' } });
  expect(submit).toBeEnabled();

  fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
  expect(input).toHaveValue('');
  expect(input).toHaveFocus();
  expect(submit).toBeDisabled();
  expect(screen.getByRole('heading', { name: 'Movies' })).toBeVisible();
  expect(searchCommand).not.toHaveBeenCalled();

  fireEvent.input(input, { target: { value: '  alien  ' } });
  expect(searchCommand).not.toHaveBeenCalled();
  fireEvent.submit(form);

  await waitFor(() =>
    expect(searchCommand).toHaveBeenCalledWith({
      limit: 24,
      query: 'alien',
      startIndex: 0,
    }),
  );
  expect(await screen.findByRole('heading', { name: 'Search results for “alien”' })).toBeVisible();
  expect(screen.getByText('25 results')).toBeVisible();
  expect(await screen.findByRole('link', { name: 'Open Alien, favorite' })).toBeVisible();

  cleanup();
});

test('library search renders compact rows with metadata and status text', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library/search?q=alien');

  await screen.findByRole('heading', { name: 'Search results for “alien”' });

  const movieRow = await screen.findByRole('link', { name: 'Open Alien, favorite' });
  expect(movieRow).toHaveAttribute('href', '/library/items/alien-movie');
  expect(within(movieRow).getByText('Movie • 1979')).toBeVisible();
  expect(within(movieRow).getByText('Played')).toBeVisible();
  expect(within(movieRow).getByText('Favorite')).toBeVisible();

  const episodeRow = screen.getByRole('link', { name: 'Open Alien Covenant: Homecoming' });
  expect(episodeRow).toHaveAttribute('href', '/library/items/alien-episode');
  expect(within(episodeRow).getByText('Alien Earth • S01E02')).toBeVisible();
  expect(within(episodeRow).getByText('Played')).toBeVisible();

  const showRow = screen.getByRole('link', { name: 'Open Alien Earth' });
  expect(showRow).toHaveAttribute('href', '/library/shows/alien-show');
  expect(within(showRow).getByText('Series • 2025')).toBeVisible();

  cleanup();
});

test('library search auto-loads the next page and retries a failed page', async () => {
  mockShellCommands();
  let nextPageShouldFail = true;
  const searchCommand = rstest
    .spyOn(commands, 'librarySearchVideo')
    .mockImplementation((request) => {
      if (request.startIndex === 24 && nextPageShouldFail) {
        return Promise.resolve({
          error: { code: 'internal', message: 'Search page failed' },
          status: 'error',
        });
      }

      return Promise.resolve({
        data: videoSearchPage(request.startIndex),
        status: 'ok',
      });
    });
  const cleanup = renderShell('/library/search?q=alien');

  await screen.findByRole('link', { name: 'Open Alien, favorite' });
  window.__TEST_INTERSECTION_OBSERVER__.trigger(true);

  await waitFor(() =>
    expect(searchCommand).toHaveBeenCalledWith({ limit: 24, query: 'alien', startIndex: 24 }),
  );
  expect(await screen.findByText('Search page failed')).toBeVisible();
  expect(screen.getByRole('link', { name: 'Open Alien, favorite' })).toBeVisible();

  nextPageShouldFail = false;
  fireEvent.click(screen.getByRole('button', { name: 'Retry loading more' }));

  expect(await screen.findByRole('link', { name: 'Open Alien Rematch' })).toBeVisible();
  await waitFor(() => expect(screen.queryByText('Search page failed')).toBeNull());

  cleanup();
});

test('library search back restores cached pages and scroll offset', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library/search?q=alien');

  const movieRow = await screen.findByRole('link', { name: 'Open Alien, favorite' });
  const viewport = appScrollViewport();
  viewport.scrollTop = 432;
  fireEvent.scroll(viewport);

  fireEvent.click(movieRow);

  expect(await screen.findByRole('heading', { name: 'Detail Movie' })).toBeVisible();
  await waitFor(() => expect(viewport.scrollTop).toBe(0));

  fireEvent.click(screen.getByRole('button', { name: 'Back' }));

  expect(await screen.findByRole('heading', { name: 'Search results for “alien”' })).toBeVisible();
  expect(await screen.findByRole('link', { name: 'Open Alien, favorite' })).toBeVisible();
  expect(screen.queryByText('Searching library')).toBeNull();
  await waitFor(() => expect(screen.queryByRole('heading', { name: 'Detail Movie' })).toBeNull());
  await waitFor(() => expect(viewport.scrollTop).toBe(432));

  cleanup();
});

test('library search redirects a whitespace-only query to library home without searching', async () => {
  mockShellCommands();
  const cleanup = renderShell('/library/search?q=%20%20');

  expect(await screen.findByRole('heading', { name: 'Continue Watching' })).toBeVisible();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(commands.librarySearchVideo).not.toHaveBeenCalled();

  cleanup();
});
