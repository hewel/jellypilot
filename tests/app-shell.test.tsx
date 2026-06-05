import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import {
  type AppConfig,
  commands,
  events,
  type NowPlayingState,
  type VideoHome,
  type VideoLibraryPage,
} from '../src/bindings';
import AuthenticatedShell, {
  type LibraryView,
} from '../src/components/AuthenticatedShell';
import { ToastProvider } from '../src/components/ToastProvider';

const connectedState = {
  connected: true,
  serverUrl: 'https://jellyfin.example.com',
  serverName: 'Jellyfin Home',
  userName: 'Ada',
};

const disconnectedState = {
  ...connectedState,
  connected: false,
};

const nowPlaying: NowPlayingState = {
  status: 'playing',
  player: {
    connected: true,
    paused: false,
    muted: false,
    timePos: 42,
    duration: 180,
    volume: 80,
  },
  media: {
    itemId: 'episode-1',
    name: 'The Pilot',
    itemType: 'Episode',
    seriesName: 'Example Show',
    seasonNumber: 1,
    episodeNumber: 1,
  },
  canPlayNext: true,
  canPlayPrevious: false,
  nextUnavailableReason: null,
  previousUnavailableReason: 'noCurrentItem',
};

const config: AppConfig = {
  deviceName: 'JMSR Test',
  mpvPath: null,
  mpvArgs: [],
  progressInterval: 5,
  startMinimized: false,
  introSkipperMode: 'automatic',
  keybindNext: 'Shift+>',
  keybindPrev: 'Shift+<',
  keybindIntroSkip: 'g',
  preferredSubtitleLanguages: [],
};

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
      played: false,
      favorite: true,
      artworkUrl: 'https://jellyfin.example.com/Items/movie-1/Images/Primary',
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
      artworkUrl: null,
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
      artworkUrl: null,
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
      artworkUrl: null,
    },
  ],
  libraryShortcuts: [
    {
      id: 'movies',
      name: 'Movies',
      collectionType: 'movies',
      itemCount: 8,
      artworkUrl: null,
    },
    {
      id: 'shows',
      name: 'Shows',
      collectionType: 'tvshows',
      itemCount: 5,
      artworkUrl: null,
    },
  ],
};

function videoLibraryPage(startIndex: number): VideoLibraryPage {
  if (startIndex === 0) {
    return {
      libraryId: 'movies',
      collectionType: 'movies',
      startIndex: 0,
      limit: 24,
      totalRecordCount: 25,
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
          artworkUrl:
            'https://jellyfin.example.com/Items/movie-1/Images/Primary',
        },
      ],
    };
  }

  return {
    libraryId: 'movies',
    collectionType: 'movies',
    startIndex,
    limit: 24,
    totalRecordCount: 25,
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
        artworkUrl: null,
      },
    ],
  };
}

function mockShellCommands(state = connectedState) {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(state);
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue(config);
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    status: 'ok',
    data: videoHome,
  });
  rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      status: 'ok',
      data: videoLibraryPage(request.startIndex),
    }),
  );
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
}

function renderShell(
  activeArea:
    | 'library'
    | 'now-playing'
    | 'settings'
    | 'diagnostics' = 'library',
  libraryView?: LibraryView,
) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <AuthenticatedShell
          activeArea={activeArea}
          libraryView={libraryView}
          onSignedOut={() => undefined}
        />
      </ToastProvider>
    ),
    root,
  );

  return () => {
    dispose();
    root.remove();
  };
}

afterEach(() => {
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});

test('authenticated shell exposes peer navigation areas', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });

  const nav = screen.getByRole('navigation', { name: 'JMSR areas' });
  const libraryLink = screen.getByRole('link', { name: 'Library' });
  expect(nav).toBeVisible();
  expect(nav).toHaveClass('overflow-x-auto');
  expect(nav).toHaveClass('lg:flex-col');
  expect(libraryLink).toHaveAttribute('aria-current', 'page');
  expect(libraryLink).toHaveClass('focus-visible:ring-2');
  expect(screen.getByRole('link', { name: 'Now Playing' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Settings' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Diagnostics' })).toBeVisible();

  cleanup();
});

test('library landing renders command-backed rows and compact now playing link', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });

  expect(
    await screen.findByRole('heading', { name: 'Continue Watching' }),
  ).toBeVisible();
  expect(screen.getByRole('link', { name: /Resume Movie/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Next Episode/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Latest Movie/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Latest Episode/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Movies/ })).toHaveAttribute(
    'href',
    '/library/movies/movies',
  );
  expect(screen.getByRole('link', { name: /Shows/ })).toHaveAttribute(
    'href',
    '/library/tvshows/shows',
  );
  expect(screen.getByAltText('Resume Movie artwork')).toHaveAttribute(
    'src',
    videoHome.continueWatching[0]?.artworkUrl ?? '',
  );
  expect(screen.getAllByText('No artwork')).toHaveLength(3);
  expect(screen.getByText('The Pilot')).toBeVisible();
  expect(
    screen.getByRole('link', { name: 'Open Now Playing' }),
  ).toHaveAttribute('href', '/now-playing');

  cleanup();
});

test('library browse loads paged results and opens detail links without playback', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell('library', {
    kind: 'browse',
    collectionType: 'movies',
    libraryId: 'movies',
  });

  await screen.findByRole('heading', { name: 'Movies' });
  expect(
    await screen.findByRole('link', { name: /Paged Movie/ }),
  ).toHaveAttribute('href', '/library/items/movie-1');
  expect(screen.getByAltText('Paged Movie artwork')).toBeVisible();
  expect(browseCommand).toHaveBeenCalledWith({
    collectionType: 'movies',
    libraryId: 'movies',
    startIndex: 0,
    limit: 24,
  });

  const movieLink = screen.getByRole('link', { name: /Paged Movie/ });
  movieLink.addEventListener('click', (event) => event.preventDefault());
  fireEvent.click(movieLink);
  expect(mpvStart).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
  expect(
    await screen.findByRole('link', { name: /Paged Movie 25/ }),
  ).toHaveAttribute('href', '/library/items/movie-25');
  expect(browseCommand).toHaveBeenLastCalledWith({
    collectionType: 'movies',
    libraryId: 'movies',
    startIndex: 24,
    limit: 24,
  });
  expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();

  cleanup();
});

test('settings shell area preserves session and configuration controls', async () => {
  mockShellCommands();
  const cleanup = renderShell('settings');

  await screen.findByRole('heading', { name: 'Connection' });
  expect(screen.getByRole('button', { name: 'Disconnect' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Sign out' })).toBeVisible();
  await waitFor(() =>
    expect(screen.getByDisplayValue('JMSR Test')).toBeVisible(),
  );
  expect(screen.getByRole('heading', { name: 'Shortcut keys' })).toBeVisible();
  expect(screen.getByRole('button', { name: /Automatic/ })).toHaveAttribute(
    'aria-pressed',
    'true',
  );

  cleanup();
});

test('diagnostics shell area preserves diagnostics panel behavior', async () => {
  mockShellCommands();
  const cleanup = renderShell('diagnostics');

  await screen.findByRole('heading', { name: 'Diagnostics' });
  expect(screen.getByText('0 sanitized runtime events')).toBeVisible();
  expect(screen.getByRole('checkbox', { name: 'Auto-scroll' })).toBeChecked();
  expect(
    screen.getByRole('button', { name: 'Copy diagnostics' }),
  ).toBeVisible();

  cleanup();
});

test('library landing exposes disconnected and retry states', async () => {
  mockShellCommands(disconnectedState);
  const videoHomeCommand = rstest.spyOn(commands, 'libraryVideoHome');
  const cleanup = renderShell();

  await screen.findByText('Library requires a live Jellyfin connection');
  expect(screen.getByRole('button', { name: 'Retry Library' })).toBeVisible();
  expect(videoHomeCommand).not.toHaveBeenCalled();

  cleanup();
});

test('library landing surfaces command errors without fake content', async () => {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(connectedState);
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    status: 'error',
    error: { code: 'network', message: 'Jellyfin unavailable' },
  });
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
  const cleanup = renderShell();

  await screen.findByText('Jellyfin unavailable');
  expect(screen.getByRole('button', { name: 'Retry Library' })).toBeVisible();
  expect(screen.queryByText('Continue Watching')).toBeNull();

  cleanup();
});

test('library landing exposes empty real-data state', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryVideoHome').mockResolvedValue({
    status: 'ok',
    data: {
      continueWatching: [],
      nextUp: [],
      latestMovies: [],
      latestEpisodes: [],
      libraryShortcuts: [],
    },
  });
  const cleanup = renderShell();

  await screen.findByText('Video Home has no video rows yet');
  expect(screen.queryByText('No artwork')).toBeNull();

  cleanup();
});

test('now playing area exposes full playback controls', async () => {
  mockShellCommands();
  const cleanup = renderShell('now-playing');

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Pause' })).toBeVisible();
  expect(screen.getByRole('slider', { name: 'Seek position' })).toBeVisible();

  cleanup();
});
