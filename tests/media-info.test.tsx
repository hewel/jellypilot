import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { screen } from '@testing-library/dom';
import { Exit, Option } from 'effect';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import type { VideoItemDetail, VideoShowDetail } from '../src/bindings';
import { MediaInfoContent, MediaInfoHoverCard } from '../src/components/library/MediaInfoHoverCard';
import { fetchMediaDetail } from '../src/effects/library';
import type { MediaDetail } from '../src/effects/library';
import { queryKeys, runExit } from '../src/effects/query';
import { createTestQueryClient, TestQueryProvider } from './query-client';

const connectedState = {
  connected: true,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userName: 'Ada',
};

const movieDetail: VideoItemDetail = {
  artworkUrl: 'https://example.com/movie.png',
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
  artworkUrl: null,
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
  artworkUrl: Option.some('https://example.com/movie.png'),
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
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(connectedState);
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

  await queryClient.fetchQuery({
    queryKey: queryKeys.libraryMediaDetail('Movie', 'movie-1'),
    queryFn: () => runExit(fetchMediaDetail('movie-1', 'Movie')),
    staleTime: Infinity,
  });
  await queryClient.fetchQuery({
    queryKey: queryKeys.libraryMediaDetail('Movie', 'movie-1'),
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
  expect(screen.getByText(/Played/)).toBeInTheDocument();
  expect(screen.getByText(/Favorite/)).toBeInTheDocument();

  dispose();
  root.remove();
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

  expect(screen.getByText('Test Movie card')).toBeInTheDocument();
  expect(itemDetail).not.toHaveBeenCalled();

  dispose();
  root.remove();
});
