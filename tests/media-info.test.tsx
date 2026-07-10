import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { Exit, Option } from 'effect';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import type { VideoItemDetail, VideoShowDetail } from '../src/bindings';
import { MediaInfoContent, MediaInfoHoverCard } from '../src/components/library/MediaInfoHoverCard';
import { fetchMediaDetail } from '../src/effects/library';
import type { MediaDetail } from '../src/effects/library';
import { librarySessionKey, queryKeys, runExit } from '../src/effects/query';
import { createTestQueryClient, TestQueryProvider } from './query-client';

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

const movieDetail: VideoItemDetail = {
  artworkImageId: 'https://example.com/movie.png',
  backdropImageId: null,
  audioStreams: [],
  canPlay: true,
  canResume: true,
  episodeNumber: null,
  favorite: true,
  genres: ['Drama', 'Sci-Fi'],
  id: 'movie-1',
  itemType: 'Movie',
  name: 'Test Movie',
  overview: 'A test movie overview.',
  played: true,
  playedPercentage: 25,
  productionYear: 2024,
  resumePositionSeconds: 120,
  runtimeSeconds: 7200,
  seasonNumber: null,
  seriesId: null,
  seriesName: null,
  subtitleStreams: [],
};

const showDetail: VideoShowDetail = {
  artworkImageId: null,
  backdropImageId: null,
  canPlay: true,
  favorite: false,
  genres: ['Crime', 'Thriller'],
  id: 'series-1',
  name: 'Test Show',
  nextEpisode: null,
  overview: 'A test show overview.',
  played: false,
  productionYear: 2022,
  seasons: [],
};

const movieMediaDetail: MediaDetail = {
  artworkImageId: Option.some('https://example.com/movie.png'),
  favorite: true,
  genres: ['Drama', 'Sci-Fi'],
  id: 'movie-1',
  itemType: 'Movie',
  name: 'Test Movie',
  overview: Option.some('A test movie overview.'),
  played: true,
  playedPercentage: Option.some(25),
  productionYear: Option.some(2024),
  resumePositionSeconds: Option.some(120),
  runtimeSeconds: Option.some(7200),
};

function mediaValue<A, E>(exit: Exit.Exit<A, E>): A | null {
  return Exit.match(exit, {
    onFailure: (): A | null => null,
    onSuccess: (value): A | null => value,
  });
}

beforeEach(() => {
  rstest.spyOn(commands, 'serverGetState').mockResolvedValue(connectedState);
});

afterEach(() => {
  rstest.restoreAllMocks();
});

test('fetchMediaDetail routes movies to item detail', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValue({ data: movieDetail, status: 'ok' });

  const result = await runExit(fetchMediaDetail('movie-1', 'Movie'));

  expect(Exit.isSuccess(result)).toBe(true);
  expect(itemDetail).toHaveBeenCalledWith('movie-1');
  expect(mediaValue(result)).toMatchObject({
    genres: ['Drama', 'Sci-Fi'],
    itemType: 'Movie',
    overview: Option.some('A test movie overview.'),
    runtimeSeconds: Option.some(7200),
  });
});

test('solid query caches media detail successes', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValue({ data: movieDetail, status: 'ok' });
  const queryClient = createTestQueryClient();
  const sessionKey = librarySessionKey(connectedState);

  await queryClient.fetchQuery({
    queryKey: queryKeys.libraryMediaDetail(sessionKey, 'Movie', 'movie-1'),
    queryFn: () => runExit(fetchMediaDetail('movie-1', 'Movie')),
    staleTime: Infinity,
  });
  await queryClient.fetchQuery({
    queryKey: queryKeys.libraryMediaDetail(sessionKey, 'Movie', 'movie-1'),
    queryFn: () => runExit(fetchMediaDetail('movie-1', 'Movie')),
    staleTime: Infinity,
  });

  expect(itemDetail).toHaveBeenCalledTimes(1);
});

test('fetchMediaDetail routes series to show detail and nulls show-only fields', async () => {
  const showCommand = rstest
    .spyOn(commands, 'libraryShowDetail')
    .mockResolvedValue({ data: showDetail, status: 'ok' });

  const result = await runExit(fetchMediaDetail('series-1', 'Series'));

  expect(showCommand).toHaveBeenCalledWith('series-1');
  expect(Exit.isSuccess(result)).toBe(true);
  expect(mediaValue(result)).toMatchObject({
    genres: ['Crime', 'Thriller'],
    itemType: 'Series',
    overview: Option.some('A test show overview.'),
    runtimeSeconds: Option.none(),
  });
});

test('fetchMediaDetail passes failures through without caching them', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValueOnce({
      error: { code: 'network', message: 'detail unavailable' },
      status: 'error',
    })
    .mockResolvedValueOnce({ data: movieDetail, status: 'ok' });

  const failed = await runExit(fetchMediaDetail('err-1', 'Movie'));
  const ok = await runExit(fetchMediaDetail('err-1', 'Movie'));

  expect(Exit.isSuccess(failed)).toBe(false);
  expect(Exit.isSuccess(ok)).toBe(true);
  expect(itemDetail).toHaveBeenCalledTimes(2);
});

test('MediaInfoContent renders overview, genres, runtime, resume, and user-data state', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <MediaInfoContent detail={movieMediaDetail} />, root);

  expect(screen.getByText('A test movie overview.')).toBeInTheDocument();
  expect(screen.getByText('Drama')).toBeInTheDocument();
  expect(screen.getByText('Sci-Fi')).toBeInTheDocument();
  expect(screen.getByText(/2h 0m/)).toBeInTheDocument();
  expect(screen.getByText(/25% watched/)).toBeInTheDocument();
  expect(screen.getByRole('progressbar', { name: 'Test Movie watch progress' })).toHaveAttribute(
    'aria-valuenow',
    '25',
  );
  expect(screen.getByText(/Played/)).toBeInTheDocument();
  expect(screen.getByText(/Favorite/)).toBeInTheDocument();

  dispose();
  root.remove();
});

test('MediaInfoHoverCard opens on hover and renders media detail', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValue({ data: movieDetail, status: 'ok' });
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <MediaInfoHoverCard id="movie-1" itemType="Movie">
          <a href="/library/items/movie-1">Test Movie card</a>
        </MediaInfoHoverCard>
      </TestQueryProvider>
    ),
    root,
  );

  try {
    const trigger = screen.getByRole('link', { name: 'Test Movie card' });
    expect(itemDetail).not.toHaveBeenCalled();
    const hoverRoot = trigger.closest('[data-part="root"]');
    expect(hoverRoot).not.toBeNull();
    fireEvent.mouseEnter(hoverRoot!);

    await waitFor(() => expect(itemDetail).toHaveBeenCalledWith('movie-1'));
    expect(await screen.findByText('A test movie overview.')).toBeInTheDocument();
    expect(screen.getByText('Drama')).toBeVisible();
    expect(screen.getByText(/2h 0m/)).toBeVisible();
    expect(screen.getByText('25% watched')).toBeVisible();
  } finally {
    dispose();
    root.remove();
  }
});

test('MediaInfoHoverCard opens on focus and closes on Escape', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValue({ data: movieDetail, status: 'ok' });
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <MediaInfoHoverCard id="movie-1" itemType="Movie">
          <a href="/library/items/movie-1">Test Movie card</a>
        </MediaInfoHoverCard>
      </TestQueryProvider>
    ),
    root,
  );

  try {
    const trigger = screen.getByRole('link', { name: 'Test Movie card' });
    fireEvent.focusIn(trigger);

    await waitFor(() => expect(itemDetail).toHaveBeenCalledWith('movie-1'));
    expect(await screen.findByText('A test movie overview.')).toBeVisible();
    fireEvent.keyDown(trigger, { key: 'Escape' });

    await waitFor(() => expect(screen.queryByText('A test movie overview.')).toBeNull());
  } finally {
    dispose();
    root.remove();
  }
});

test('MediaInfoHoverCard shows an error state when detail loading fails', async () => {
  rstest.spyOn(commands, 'libraryItemDetail').mockResolvedValueOnce({
    error: { code: 'network', message: 'Could not load detail' },
    status: 'error',
  });
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <MediaInfoHoverCard id="movie-1" itemType="Movie">
          <a href="/library/items/movie-1">Test Movie card</a>
        </MediaInfoHoverCard>
      </TestQueryProvider>
    ),
    root,
  );

  try {
    const trigger = screen.getByRole('link', { name: 'Test Movie card' });
    const hoverRoot = trigger.closest('[data-part="root"]');
    expect(hoverRoot).not.toBeNull();
    fireEvent.mouseEnter(hoverRoot!);

    expect(await screen.findByText('Could not load detail')).toBeInTheDocument();
  } finally {
    dispose();
    root.remove();
  }
});

test('MediaInfoHoverCard renders trigger children and does not fetch before opening', () => {
  const itemDetail = rstest.spyOn(commands, 'libraryItemDetail');
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <MediaInfoHoverCard id="movie-1" itemType="Movie">
          <a href="/library/items/movie-1">Test Movie card</a>
        </MediaInfoHoverCard>
      </TestQueryProvider>
    ),
    root,
  );

  try {
    expect(screen.getByText('Test Movie card')).toBeInTheDocument();
    expect(itemDetail).not.toHaveBeenCalled();
  } finally {
    dispose();
    root.remove();
  }
});
