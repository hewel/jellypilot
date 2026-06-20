import { afterEach, expect, rstest, test } from '@rstest/core';
import { RouterProvider, createMemoryHistory } from '@tanstack/solid-router';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands, events } from '../src/bindings';
import type {
  AppConfig,
  NowPlayingState,
  VideoHome,
  VideoItemDetail,
  VideoLibraryPage,
  VideoLibraryShortcut,
  VideoSeasonEpisodes,
  VideoShowDetail,
} from '../src/bindings';
import { ToastProvider } from '../src/components/ToastProvider';
import { createJmsrRouter } from '../src/router';

// Mock scrollTo since JSDOM doesn't implement layout/scrolling APIs
Element.prototype.scrollTo = () => {};
window.scrollTo = () => {};

const connectedState = {
  connected: true,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userName: 'Ada',
};

const disconnectedState = {
  ...connectedState,
  connected: false,
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
  deviceName: 'JMSR Test',
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
      played: false,
      favorite: true,
      artworkUrl: 'https://jellyfin.example.com/Items/movie-1/Images/Primary',
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
};

const videoLibraryShortcuts: VideoLibraryShortcut[] = [
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
];

const movieDetail: VideoItemDetail = {
  artworkUrl: 'https://jellyfin.example.com/Items/detail-movie/Images/Primary',
  audioStreams,
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
  subtitleStreams,
};

const episodeDetail: VideoItemDetail = {
  artworkUrl: null,
  audioStreams,
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
  artworkUrl: null,
  canPlay: true,
  favorite: false,
  genres: ['Drama'],
  id: 'series-1',
  name: 'Example Show',
  nextEpisode: {
    artworkUrl: null,
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
          artworkUrl: 'https://jellyfin.example.com/Items/movie-1/Images/Primary',
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
        artworkUrl: null,
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

function mockShellCommands(state = connectedState) {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(true);
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(state);
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
  rstest.spyOn(commands, 'libraryBrowseVideo').mockImplementation((request) =>
    Promise.resolve({
      data: videoLibraryPage(request.startIndex),
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

function renderShell(path = '/library') {
  const root = document.createElement('div');
  document.body.append(root);
  const router = createJmsrRouter(createMemoryHistory({ initialEntries: [path] }));
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
    .find((element): element is HTMLSelectElement => element.tagName === 'SELECT');

  if (!select) {
    throw new Error(`Could not find hidden Ark select for ${label}`);
  }

  return select;
}

function getArkCombobox(label: string) {
  const combobox = screen
    .getAllByLabelText(label)
    .find((element): element is HTMLButtonElement => element.getAttribute('role') === 'combobox');

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

  await screen.findByRole('navigation', { name: 'Library navigation' });

  const nav = screen.getByRole('navigation', { name: 'JMSR areas' });
  const libraryLink = screen.getByRole('link', { name: 'Library' });
  expect(nav).toBeVisible();
  expect(nav).toHaveClass('overflow-x-auto');
  expect(nav).toHaveClass('lg:overflow-visible');
  expect(libraryLink).toHaveAttribute('aria-current', 'page');
  expect(libraryLink).toHaveClass('focus-visible:ring-2');
  expect(screen.queryByRole('link', { name: 'Now Playing' })).toBeNull();
  expect(screen.getByRole('button', { name: /Now Playing: Playing — The Pilot/ })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Settings' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Diagnostics' })).toBeVisible();

  cleanup();
});

test('library landing renders command-backed rows and drawer trigger', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Library navigation' });

  const navigation = screen.getByRole('navigation', { name: 'Library navigation' });
  expect(navigation).toBeVisible();
  expect(navigation).toHaveClass('sticky');
  expect(screen.getByRole('radio', { name: 'Home' })).toBeChecked();
  expect(screen.getByRole('radio', { name: 'Movies' })).toBeVisible();
  expect(screen.getByRole('radio', { name: 'Shows' })).toBeVisible();
  expect(await screen.findByRole('heading', { name: 'Continue Watching' })).toBeVisible();
  expect(screen.getByRole('link', { name: /Resume Movie/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Next Episode/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Latest Movie/ })).toBeVisible();
  expect(screen.getByRole('link', { name: /Latest Episode/ })).toBeVisible();
  const resumeArtwork = screen.getByAltText('Resume Movie artwork');
  expect(resumeArtwork).toHaveAttribute('src', videoHome.continueWatching[0]?.artworkUrl ?? '');
  expect(resumeArtwork.parentElement).toHaveClass('aspect-video');
  fireEvent.load(resumeArtwork);
  expect(resumeArtwork.parentElement).toHaveClass('aspect-video');
  const latestMovieLink = screen.getByRole('link', { name: /Latest Movie/ });
  expect(
    [...latestMovieLink.querySelectorAll('div')].some((node) =>
      node.className.includes('aspect-[2/3]'),
    ),
  ).toBe(true);
  expect(screen.getAllByText('No artwork')).toHaveLength(3);
  expect(screen.getByRole('button', { name: /Now Playing: Playing — The Pilot/ })).toBeVisible();

  cleanup();
});

test('library browse loads paged results and opens detail links without playback', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const mpvStart = rstest.spyOn(commands, 'mpvStart');
  const cleanup = renderShell('/library/movies/movies');

  const navigation = await screen.findByRole('navigation', { name: 'Library navigation' });
  expect(navigation).toBeVisible();
  expect(screen.getByRole('radio', { name: 'Movies' })).toBeChecked();
  expect(screen.getByRole('radio', { name: 'Home' })).toBeVisible();
  expect(screen.getByRole('radio', { name: 'Shows' })).toBeVisible();
  await screen.findByRole('heading', { name: 'Movies' });
  expect(await screen.findByRole('link', { name: /Paged Movie/ })).toHaveAttribute(
    'href',
    '/library/items/movie-1',
  );
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

  const movieLink = screen.getByRole('link', { name: /Paged Movie/ });
  movieLink.addEventListener('click', (event) => event.preventDefault());
  fireEvent.click(movieLink);
  expect(mpvStart).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
  expect(await screen.findByRole('link', { name: /Paged Movie 25/ })).toHaveAttribute(
    'href',
    '/library/items/movie-25',
  );
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

test('library browse controls reload paged results from the first page', async () => {
  mockShellCommands();
  const browseCommand = rstest.spyOn(commands, 'libraryBrowseVideo');
  const cleanup = renderShell('/library/movies/movies');

  await screen.findByRole('link', { name: /Paged Movie/ });
  fireEvent.click(screen.getByRole('button', { name: 'Sort By' }));
  fireEvent.click(screen.getByText('Recently added', { selector: 'span' }));

  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'all',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );

  fireEvent.click(screen.getByRole('button', { name: 'Status' }));
  fireEvent.click(screen.getByText('Unplayed', { selector: 'span' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'movies',
      favoritesOnly: false,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'unplayed',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );

  fireEvent.click(screen.getByRole('button', { name: 'Status' }));
  fireEvent.click(screen.getByText('Favorites Only', { selector: 'span' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
      collectionType: 'movies',
      favoritesOnly: true,
      libraryId: 'movies',
      limit: 24,
      playedFilter: 'unplayed',
      sort: 'recentlyAdded',
      startIndex: 0,
    }),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Sort ascending' }));
  await waitFor(() =>
    expect(browseCommand).toHaveBeenLastCalledWith({
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
  expect(screen.getByRole('button', { name: 'Play from beginning' })).toBeVisible();
  expect(getArkHiddenSelect('Audio track')).toHaveValue('auto');
  expect(getArkHiddenSelect('Subtitle track')).toHaveValue('auto');
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
  expect(screen.queryByRole('button', { name: 'Resume playback' })).toBeNull();
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Play from beginning' })).not.toBeDisabled(),
  );
  await selectArkOption('Audio track', /Japanese - FLAC/);
  await selectArkOption('Subtitle track', /English - SRT/);
  fireEvent.click(screen.getByRole('button', { name: 'Play from beginning' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      audioStreamIndex: 2,
      itemId: 'detail-movie',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: 3,
    }),
  );
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Play from beginning' })).not.toBeDisabled(),
  );
  await selectArkOption('Subtitle track', 'Off');
  fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      audioStreamIndex: 2,
      itemId: 'detail-movie',
      mode: 'resume',
      startPositionSeconds: 120,
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
      audioStreamIndex: null,
      itemId: 'detail-episode',
      mode: 'start',
      startPositionSeconds: 0,
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
      action: 'favorite',
      itemId: 'series-1',
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
      audioStreamIndex: 2,
      itemId: 'episode-2',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: null,
    }),
  );
  await waitFor(() => expect(screen.queryAllByLabelText('Audio track')).toHaveLength(0));

  // Next episode link
  expect(screen.getByRole('link', { name: 'Next: Next Episode' })).toHaveAttribute(
    'href',
    '/library/items/episode-2',
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
  await waitFor(() => expect(getArkCombobox('Subtitle track')).toBeVisible());
  await selectArkOption('Subtitle track', 'Off');
  fireEvent.click(screen.getByRole('button', { name: 'Start playback' }));
  await waitFor(() =>
    expect(playCommand).toHaveBeenLastCalledWith({
      audioStreamIndex: 1,
      itemId: 'episode-2',
      mode: 'start',
      startPositionSeconds: 0,
      subtitleStreamIndex: -1,
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

  expect(mpvStart).not.toHaveBeenCalled();

  cleanup();
});

test('settings shell area preserves session and configuration controls', async () => {
  mockShellCommands();
  const cleanup = renderShell('/settings');

  await screen.findByRole('heading', { name: 'Connection' });
  expect(screen.getByRole('button', { name: 'Disconnect' })).toBeVisible();
  expect(screen.getByRole('button', { name: 'Sign out' })).toBeVisible();
  await waitFor(() => expect(screen.getByDisplayValue('JMSR Test')).toBeVisible());
  expect(screen.getByRole('heading', { name: 'Shortcut keys' })).toBeVisible();
  expect(screen.getByRole('button', { name: /Automatic/ })).toHaveAttribute('aria-pressed', 'true');

  cleanup();
});

test('diagnostics shell area preserves diagnostics panel behavior', async () => {
  mockShellCommands();
  const cleanup = renderShell('/diagnostics');

  await screen.findByRole('heading', { name: 'Diagnostics' });
  expect(screen.getByText('0 sanitized runtime events')).toBeVisible();
  expect(screen.getByRole('checkbox', { name: 'Auto-scroll' })).toBeChecked();
  expect(screen.getByRole('button', { name: 'Copy diagnostics' })).toBeVisible();

  cleanup();
});

test('library landing has no retry and skips video home when disconnected', async () => {
  mockShellCommands(disconnectedState);
  const videoHomeCommand = rstest.spyOn(commands, 'libraryVideoHome');
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Library navigation' });
  expect(screen.queryByRole('button', { name: 'Retry Library' })).toBeNull();
  expect(videoHomeCommand).not.toHaveBeenCalled();

  cleanup();
});

test('library landing renders no fake content on command error', async () => {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(true);
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(connectedState);
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

  await screen.findByRole('navigation', { name: 'Library navigation' });
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

  await screen.findByRole('navigation', { name: 'Library navigation' });
  expect(screen.queryByRole('button', { name: 'Retry Library' })).toBeNull();
  expect(screen.queryByText('No artwork')).toBeNull();

  cleanup();
});

test('now playing drawer exposes full playback controls', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('navigation', { name: 'Library navigation' });

  const trigger = screen.getByRole('button', { name: /Now Playing: Playing — The Pilot/ });
  fireEvent.click(trigger);

  const dialog = await screen.findByRole('dialog', { name: 'Now Playing' });
  expect(dialog).toBeVisible();
  expect(screen.getByText('The Pilot')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Pause' })).toBeVisible();
  expect(screen.getByRole('slider', { name: 'Seek position' })).toBeVisible();

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
