import { afterEach, expect, rstest, test } from '@rstest/core';
import { createMemoryHistory, RouterProvider } from '@tanstack/solid-router';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import {
  type AppConfig,
  commands,
  events,
  type NowPlayingState,
  type VideoHome,
  type VideoItemDetail,
  type VideoLibraryPage,
  type VideoSearchPage,
  type VideoSeasonEpisodes,
  type VideoShowDetail,
} from '../src/bindings';
import { ToastProvider } from '../src/components/ToastProvider';
import { createJmsrRouter } from '../src/router';

// Mock scrollTo since JSDOM doesn't implement layout/scrolling APIs
Element.prototype.scrollTo = () => {};
window.scrollTo = () => {};

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

const audioStreams = [
  {
    index: 1,
    label: 'English - AAC 2.0',
    language: 'eng',
    codec: 'aac',
    isDefault: true,
    isExternal: false,
  },
  {
    index: 2,
    label: 'Japanese - FLAC 5.1',
    language: 'jpn',
    codec: 'flac',
    isDefault: false,
    isExternal: false,
  },
];

const subtitleStreams = [
  {
    index: 3,
    label: 'English - SRT',
    language: 'eng',
    codec: 'srt',
    isDefault: false,
    isExternal: true,
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

const movieDetail: VideoItemDetail = {
  id: 'detail-movie',
  name: 'Detail Movie',
  itemType: 'Movie',
  overview: 'A movie overview.',
  productionYear: 2024,
  runtimeSeconds: 7200,
  seriesId: null,
  seriesName: null,
  seasonNumber: null,
  episodeNumber: null,
  genres: ['Drama', 'Mystery'],
  played: false,
  favorite: true,
  playedPercentage: 25,
  resumePositionSeconds: 120,
  canResume: true,
  canPlay: true,
  artworkUrl: 'https://jellyfin.example.com/Items/detail-movie/Images/Primary',
  audioStreams,
  subtitleStreams,
};

const episodeDetail: VideoItemDetail = {
  id: 'detail-episode',
  name: 'Detail Episode',
  itemType: 'Episode',
  overview: null,
  productionYear: null,
  runtimeSeconds: null,
  seriesId: 'series-1',
  seriesName: 'Example Show',
  seasonNumber: 2,
  episodeNumber: 3,
  genres: ['Sci-Fi'],
  played: true,
  favorite: false,
  playedPercentage: 100,
  resumePositionSeconds: 0,
  canResume: false,
  canPlay: true,
  artworkUrl: null,
  audioStreams,
  subtitleStreams,
};

const nextEpisodeDetail: VideoItemDetail = {
  ...episodeDetail,
  id: 'episode-2',
  name: 'Next Episode',
  seasonNumber: 1,
  episodeNumber: 2,
  played: false,
  playedPercentage: null,
  resumePositionSeconds: null,
  canResume: false,
};

const showDetail: VideoShowDetail = {
  id: 'series-1',
  name: 'Example Show',
  overview: 'A show overview.',
  productionYear: 2023,
  genres: ['Drama'],
  played: false,
  favorite: false,
  canPlay: true,
  artworkUrl: null,
  nextEpisode: {
    id: 'episode-2',
    name: 'Next Episode',
    itemType: 'Episode',
    productionYear: null,
    runtimeSeconds: null,
    played: false,
    favorite: false,
    artworkUrl: null,
    seasonNumber: 1,
    episodeNumber: 2,
    seriesId: 'series-1',
    seriesName: 'Example Show',
    resumePositionSeconds: null,
    playedPercentage: null,
  },
  seasons: [
    {
      id: 'season-1',
      name: 'Season 1',
      seasonNumber: 1,
      played: false,
      favorite: false,
      artworkUrl: null,
    },
    {
      id: 'season-2',
      name: 'Season 2',
      seasonNumber: 2,
      played: false,
      favorite: true,
      artworkUrl: null,
    },
  ],
};

const seasonEpisodes: VideoSeasonEpisodes = {
  seriesId: 'series-1',
  seasonId: 'season-1',
  seasonNumber: 1,
  episodes: [
    {
      id: 'episode-2',
      name: 'Next Episode',
      itemType: 'Episode',
      productionYear: null,
      runtimeSeconds: 1800,
      played: false,
      favorite: false,
      artworkUrl: null,
      seasonNumber: 1,
      episodeNumber: 2,
      seriesId: 'series-1',
      seriesName: 'Example Show',
      resumePositionSeconds: null,
      playedPercentage: null,
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
          seasonNumber: null,
          episodeNumber: null,
          seriesId: null,
          seriesName: null,
          resumePositionSeconds: null,
          playedPercentage: null,
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
        seasonNumber: null,
        episodeNumber: null,
        seriesId: null,
        seriesName: null,
        resumePositionSeconds: null,
        playedPercentage: null,
      },
    ],
  };
}

function videoSearchPage(query: string, startIndex: number): VideoSearchPage {
  if (startIndex === 0) {
    return {
      query,
      startIndex: 0,
      limit: 24,
      totalRecordCount: 25,
      hasMore: true,
      items: [
        {
          id: 'search-movie-1',
          name: 'Search Movie',
          itemType: 'Movie',
          productionYear: 2024,
          runtimeSeconds: 7200,
          played: false,
          favorite: false,
          artworkUrl: null,
          seasonNumber: null,
          episodeNumber: null,
          seriesId: null,
          seriesName: null,
          resumePositionSeconds: null,
          playedPercentage: null,
        },
        {
          id: 'search-show-1',
          name: 'Search Show',
          itemType: 'Series',
          productionYear: null,
          runtimeSeconds: null,
          played: false,
          favorite: true,
          artworkUrl: null,
          seasonNumber: null,
          episodeNumber: null,
          seriesId: null,
          seriesName: null,
          resumePositionSeconds: null,
          playedPercentage: null,
        },
      ],
    };
  }

  return {
    query,
    startIndex,
    limit: 24,
    totalRecordCount: 25,
    hasMore: false,
    items: [
      {
        id: 'search-episode-25',
        name: 'Search Episode 25',
        itemType: 'Episode',
        productionYear: null,
        runtimeSeconds: null,
        played: false,
        favorite: false,
        artworkUrl: null,
        seasonNumber: null,
        episodeNumber: null,
        seriesId: null,
        seriesName: null,
        resumePositionSeconds: null,
        playedPercentage: null,
      },
    ],
  };
}

function mockShellCommands(state = connectedState) {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(true);
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
  rstest.spyOn(commands, 'librarySearchVideo').mockImplementation((request) =>
    Promise.resolve({
      status: 'ok',
      data: videoSearchPage(request.query, request.startIndex),
    }),
  );
  rstest.spyOn(commands, 'libraryItemDetail').mockImplementation((itemId) => {
    const data =
      itemId === 'detail-episode'
        ? episodeDetail
        : itemId === 'episode-2'
          ? nextEpisodeDetail
          : movieDetail;

    return Promise.resolve({ status: 'ok', data });
  });
  rstest.spyOn(commands, 'libraryShowDetail').mockResolvedValue({
    status: 'ok',
    data: showDetail,
  });
  rstest.spyOn(commands, 'librarySeasonEpisodes').mockResolvedValue({
    status: 'ok',
    data: seasonEpisodes,
  });
  rstest.spyOn(commands, 'libraryPlay').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  rstest.spyOn(commands, 'libraryUpdateUserData').mockResolvedValue({
    status: 'ok',
    data: { itemId: 'detail-movie', played: false, favorite: false },
  });
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
}

function renderShell(path = '/library') {
  const root = document.createElement('div');
  document.body.append(root);
  const router = createJmsrRouter(
    createMemoryHistory({ initialEntries: [path] }),
  );
  const dispose = render(
    () => (
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    ),
    root,
  );

  return () => {
    dispose();
    root.remove();
  };
}

function getArkHiddenSelect(label: string) {
  const select = screen
    .getAllByLabelText(label)
    .find(
      (element): element is HTMLSelectElement => element.tagName === 'SELECT',
    );

  if (!select) {
    throw new Error(`Could not find hidden Ark select for ${label}`);
  }

  return select;
}

function getArkCombobox(label: string) {
  const combobox = screen
    .getAllByLabelText(label)
    .find(
      (element): element is HTMLButtonElement =>
        element.getAttribute('role') === 'combobox',
    );

  if (!combobox) {
    throw new Error(`Could not find Ark combobox for ${label}`);
  }

  return combobox;
}

async function selectArkOption(label: string, name: RegExp | string) {
  fireEvent.click(getArkCombobox(label));
  fireEvent.click(await screen.findByRole('option', { name }));
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
  expect(nav).toHaveClass('lg:overflow-visible');
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
  expect(await screen.findByText('The Pilot')).toBeVisible();
  expect(
    screen.getByRole('link', { name: 'Open Now Playing' }),
  ).toHaveAttribute('href', '/now-playing');

  cleanup();
});

test('library search loads paged video results and opens detail links without playback', async () => {
  mockShellCommands();
  const searchCommand = rstest.spyOn(commands, 'librarySearchVideo');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });
  fireEvent.input(screen.getByLabelText('Search video library'), {
    target: { value: 'pilot' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Search' }));

  expect(
    await screen.findByRole('link', { name: /Search Movie/ }),
  ).toHaveAttribute('href', '/library/items/search-movie-1');
  expect(screen.getByRole('link', { name: /Search Show/ })).toHaveAttribute(
    'href',
    '/library/shows/search-show-1',
  );
  expect(searchCommand).toHaveBeenCalledWith({
    query: 'pilot',
    startIndex: 0,
    limit: 24,
  });

  const movieLink = screen.getByRole('link', { name: /Search Movie/ });
  movieLink.addEventListener('click', (event) => event.preventDefault());
  fireEvent.click(movieLink);
  expect(mpvStart).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole('button', { name: 'Load more results' }));
  expect(
    await screen.findByRole('link', { name: /Search Episode 25/ }),
  ).toHaveAttribute('href', '/library/items/search-episode-25');
  expect(searchCommand).toHaveBeenLastCalledWith({
    query: 'pilot',
    startIndex: 24,
    limit: 24,
  });

  cleanup();
});

test('library search exposes empty results and command errors with retry', async () => {
  mockShellCommands();
  const searchCommand = rstest
    .spyOn(commands, 'librarySearchVideo')
    .mockResolvedValueOnce({
      status: 'ok',
      data: {
        query: 'missing',
        startIndex: 0,
        limit: 24,
        totalRecordCount: 0,
        hasMore: false,
        items: [],
      },
    })
    .mockResolvedValueOnce({
      status: 'error',
      error: { code: 'network', message: 'Search unavailable' },
    })
    .mockResolvedValueOnce({
      status: 'ok',
      data: videoSearchPage('missing', 0),
    });
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });
  fireEvent.input(screen.getByLabelText('Search video library'), {
    target: { value: 'missing' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Search' }));
  await screen.findByText('No video search results');

  fireEvent.click(screen.getByRole('button', { name: 'Search' }));
  await screen.findByText('Search unavailable');
  fireEvent.click(screen.getByRole('button', { name: 'Retry Search' }));
  expect(
    await screen.findByRole('link', { name: /Search Movie/ }),
  ).toBeVisible();
  expect(searchCommand).toHaveBeenCalledTimes(3);

  cleanup();
});

test('library search stays disconnected without calling search command', async () => {
  mockShellCommands(disconnectedState);
  const searchCommand = rstest.spyOn(commands, 'librarySearchVideo');
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });
  fireEvent.input(screen.getByLabelText('Search video library'), {
    target: { value: 'pilot' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Search' }));

  await screen.findByText('Library requires a live Jellyfin connection');
  expect(screen.getByRole('button', { name: 'Retry Search' })).toBeVisible();
  expect(searchCommand).not.toHaveBeenCalled();

  cleanup();
});

test('library browse loads paged results and opens detail links without playback', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell('/library/movies/movies');

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
    sort: 'title',
    playedFilter: 'all',
    favoritesOnly: false,
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
    sort: 'title',
    playedFilter: 'all',
    favoritesOnly: false,
  });
  expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();

  cleanup();
});

test('library browse controls reload paged results from the first page', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('link', { name: /Paged Movie/ });
  fireEvent.click(screen.getByRole('combobox', { name: 'Sort By' }));
  fireEvent.click(screen.getByText('Recently added', { selector: 'span' }));

  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'movies',
      libraryId: 'movies',
      startIndex: 0,
      limit: 24,
      sort: 'recentlyAdded',
      playedFilter: 'all',
      favoritesOnly: false,
    }),
  );

  fireEvent.click(screen.getByRole('button', { name: 'Unplayed' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'movies',
      libraryId: 'movies',
      startIndex: 0,
      limit: 24,
      sort: 'recentlyAdded',
      playedFilter: 'unplayed',
      favoritesOnly: false,
    }),
  );

  fireEvent.click(screen.getByRole('checkbox', { name: 'Favorites Only' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'movies',
      libraryId: 'movies',
      startIndex: 0,
      limit: 24,
      sort: 'recentlyAdded',
      playedFilter: 'unplayed',
      favoritesOnly: true,
    }),
  );

  cleanup();
});

test('library browse surfaces backend sort and filter errors', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryBrowseVideo').mockResolvedValue({
    status: 'error',
    error: { code: 'internal', message: 'Unsupported library filter' },
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
  expect(screen.getByText('A movie overview.')).toBeVisible();
  expect(screen.getByText('Drama')).toBeVisible();
  expect(screen.getByText('Mystery')).toBeVisible();
  expect(screen.getByText('Favorite')).toBeVisible();
  expect(screen.getByText('2h 0m')).toBeVisible();
  expect(screen.getByText('Resume at 120s · 25% watched')).toBeVisible();
  expect(screen.getByAltText('Detail Movie artwork')).toHaveAttribute(
    'src',
    movieDetail.artworkUrl ?? '',
  );
  expect(screen.getByRole('button', { name: 'Resume' })).toBeVisible();
  expect(
    screen.getByRole('button', { name: 'Play from beginning' }),
  ).toBeVisible();
  expect(getArkHiddenSelect('Audio track')).toHaveValue('auto');
  expect(getArkHiddenSelect('Subtitle track')).toHaveValue('auto');
  fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      itemId: 'detail-movie',
      mode: 'resume',
      startPositionSeconds: 120,
      audioStreamIndex: null,
      subtitleStreamIndex: null,
    }),
  );
  expect(screen.queryByRole('button', { name: 'Resume playback' })).toBeNull();
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Play from beginning' }),
    ).not.toBeDisabled(),
  );
  await selectArkOption('Audio track', /Japanese - FLAC/);
  await selectArkOption('Subtitle track', /English - SRT/);
  fireEvent.click(screen.getByRole('button', { name: 'Play from beginning' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      itemId: 'detail-movie',
      mode: 'start',
      startPositionSeconds: 0,
      audioStreamIndex: 2,
      subtitleStreamIndex: 3,
    }),
  );
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Play from beginning' }),
    ).not.toBeDisabled(),
  );
  await selectArkOption('Subtitle track', 'Off');
  fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      itemId: 'detail-movie',
      mode: 'resume',
      startPositionSeconds: 120,
      audioStreamIndex: 2,
      subtitleStreamIndex: -1,
    }),
  );
  expect(mpvStart).not.toHaveBeenCalled();

  cleanup();
});

test('library item detail refreshes user data only after mutation success', async () => {
  mockShellCommands();
  const updateCommand = rstest.spyOn(commands, 'libraryUpdateUserData');
  rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValueOnce({ status: 'ok', data: movieDetail })
    .mockResolvedValueOnce({
      status: 'ok',
      data: { ...movieDetail, favorite: false },
    });
  const cleanup = renderShell('/library/items/detail-movie');

  await screen.findByRole('heading', { name: 'Detail Movie' });
  fireEvent.click(screen.getByRole('button', { name: 'Unfavorite' }));

  await waitFor(() =>
    expect(updateCommand).toHaveBeenCalledWith({
      itemId: 'detail-movie',
      action: 'unfavorite',
    }),
  );
  expect(await screen.findByText('Not favorite')).toBeVisible();

  cleanup();
});

test('library item detail keeps previous user data visible on mutation failure', async () => {
  mockShellCommands();
  rstest.spyOn(commands, 'libraryUpdateUserData').mockResolvedValue({
    status: 'error',
    error: { code: 'network', message: 'Favorite update failed' },
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
  expect(screen.getByText('Example Show · S02E03')).toBeVisible();
  expect(screen.getByText('Played')).toBeVisible();
  expect(screen.getByText('Not favorite')).toBeVisible();
  expect(screen.getByText('No episode artwork')).toBeVisible();
  expect(screen.getByText('View series')).toBeVisible();
  expect(screen.getByText('Sci-Fi')).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Resume' })).toBeNull();
  expect(getArkHiddenSelect('Audio track')).toHaveValue('auto');
  expect(getArkHiddenSelect('Subtitle track')).toHaveValue('auto');
  await selectArkOption('Subtitle track', 'Off');
  fireEvent.click(screen.getByRole('button', { name: 'Play' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      itemId: 'detail-episode',
      mode: 'start',
      startPositionSeconds: 0,
      audioStreamIndex: null,
      subtitleStreamIndex: -1,
    }),
  );
  expect(screen.queryByRole('button', { name: 'Start playback' })).toBeNull();

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
  expect(screen.getByText('A show overview.')).toBeVisible();
  expect(screen.getByText('Drama')).toBeVisible();
  expect(screen.getByText('Unplayed')).toBeVisible();
  expect(screen.getByText('Not favorite')).toBeVisible();

  // Series user data controls
  fireEvent.click(screen.getByRole('button', { name: 'Favorite' }));
  await waitFor(() =>
    expect(updateCommand).toHaveBeenCalledWith({
      itemId: 'series-1',
      action: 'favorite',
    }),
  );

  // Secondary "Play next episode" shortcut
  fireEvent.click(screen.getByRole('button', { name: 'Play Next Episode' }));
  await waitFor(() => expect(itemCommand).toHaveBeenCalledWith('episode-2'));
  expect(playCommand).not.toHaveBeenCalled();
  await waitFor(() => expect(getArkCombobox('Audio track')).toBeVisible());
  await selectArkOption('Audio track', /Japanese - FLAC/);
  fireEvent.click(screen.getByRole('button', { name: 'Start playback' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenCalledWith({
      itemId: 'episode-2',
      mode: 'start',
      startPositionSeconds: 0,
      audioStreamIndex: 2,
      subtitleStreamIndex: null,
    }),
  );
  await waitFor(() =>
    expect(screen.queryAllByLabelText('Audio track')).toHaveLength(0),
  );

  // Next episode link
  expect(
    screen.getByRole('link', { name: 'Next: Next Episode' }),
  ).toHaveAttribute('href', '/library/items/episode-2');

  // Season selector buttons
  expect(screen.getByRole('button', { name: 'Season 1' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Season 2' })).toBeVisible();
  expect(showCommand).toHaveBeenCalledWith('series-1');

  // Auto-load: nextEpisode.seasonNumber=1, so season 1 loads automatically
  await waitFor(() =>
    expect(seasonCommand).toHaveBeenCalledWith({
      seriesId: 'series-1',
      seasonId: 'season-1',
      seasonNumber: 1,
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
  await waitFor(() =>
    expect(itemCommand).toHaveBeenLastCalledWith('episode-2'),
  );
  await waitFor(() => expect(getArkCombobox('Subtitle track')).toBeVisible());
  await selectArkOption('Subtitle track', 'Off');
  fireEvent.click(screen.getByRole('button', { name: 'Start playback' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      itemId: 'episode-2',
      mode: 'start',
      startPositionSeconds: 0,
      audioStreamIndex: 1,
      subtitleStreamIndex: -1,
    }),
  );

  // Manual season switch
  fireEvent.click(screen.getByRole('button', { name: 'Season 2' }));
  await waitFor(() =>
    expect(seasonCommand).toHaveBeenLastCalledWith({
      seriesId: 'series-1',
      seasonId: 'season-2',
      seasonNumber: 2,
    }),
  );

  expect(mpvStart).not.toHaveBeenCalled();

  cleanup();
});

test('settings shell area preserves session and configuration controls', async () => {
  mockShellCommands();
  const cleanup = renderShell('/settings');

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
  const cleanup = renderShell('/diagnostics');

  await screen.findByRole('heading', { name: 'Diagnostics' });
  expect(screen.getByText('0 sanitized runtime events')).toBeVisible();
  expect(screen.getByRole('checkbox', { name: 'Auto-scroll' })).toBeChecked();
  expect(
    screen.getByRole('button', { name: 'Copy diagnostics' }),
  ).toBeVisible();

  cleanup();
});

test('library landing keeps retry and skips video home when disconnected', async () => {
  mockShellCommands(disconnectedState);
  const videoHomeCommand = rstest.spyOn(commands, 'libraryVideoHome');
  const cleanup = renderShell();

  expect(
    await screen.findByRole('button', { name: 'Retry Library' }),
  ).toBeVisible();
  expect(videoHomeCommand).not.toHaveBeenCalled();

  cleanup();
});

test('library landing renders no fake content on command error', async () => {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(true);
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

  expect(
    await screen.findByRole('button', { name: 'Retry Library' }),
  ).toBeVisible();
  expect(screen.queryByText('Continue Watching')).toBeNull();

  cleanup();
});

test('library landing renders no rows for empty video home', async () => {
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

  expect(
    await screen.findByRole('button', { name: 'Retry Library' }),
  ).toBeVisible();
  expect(screen.queryByText('No artwork')).toBeNull();

  cleanup();
});

test('now playing area exposes full playback controls', async () => {
  mockShellCommands();
  const cleanup = renderShell('/now-playing');

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Pause' })).toBeVisible();
  expect(screen.getByRole('slider', { name: 'Seek position' })).toBeVisible();

  cleanup();
});
