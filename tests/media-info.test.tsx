import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { screen } from '@testing-library/dom';
import { Exit } from 'effect';
import { render } from 'solid-js/web';
import {
  commands,
  type VideoItemDetail,
  type VideoShowDetail,
} from '../src/bindings';
import {
  MediaInfoContent,
  MediaInfoHoverCard,
} from '../src/components/library/MediaInfoHoverCard';
import {
  clearMediaDetailCache,
  fetchMediaDetail,
  type MediaDetail,
} from '../src/effects/library';

const connectedState = {
  connected: true,
  serverUrl: 'https://jellyfin.example.com',
  serverName: 'Jellyfin Home',
  userName: 'Ada',
};

const movieDetail: VideoItemDetail = {
  id: 'movie-1',
  name: 'Test Movie',
  itemType: 'Movie',
  overview: 'A test movie overview.',
  productionYear: 2024,
  runtimeSeconds: 7200,
  seriesId: null,
  seriesName: null,
  seasonNumber: null,
  episodeNumber: null,
  genres: ['Drama', 'Sci-Fi'],
  played: true,
  favorite: true,
  playedPercentage: 25,
  resumePositionSeconds: 120,
  canResume: true,
  canPlay: true,
  artworkUrl: 'https://example.com/movie.png',
  audioStreams: [],
  subtitleStreams: [],
};

const showDetail: VideoShowDetail = {
  id: 'series-1',
  name: 'Test Show',
  overview: 'A test show overview.',
  productionYear: 2022,
  genres: ['Crime', 'Thriller'],
  played: false,
  favorite: false,
  canPlay: true,
  artworkUrl: null,
  nextEpisode: null,
  seasons: [],
};

const movieMediaDetail: MediaDetail = {
  id: 'movie-1',
  name: 'Test Movie',
  itemType: 'Movie',
  overview: 'A test movie overview.',
  productionYear: 2024,
  runtimeSeconds: 7200,
  genres: ['Drama', 'Sci-Fi'],
  played: true,
  favorite: true,
  playedPercentage: 25,
  resumePositionSeconds: 120,
  artworkUrl: 'https://example.com/movie.png',
};

function mediaValue<A, E>(exit: Exit.Exit<A, E>): A | null {
  return Exit.match(exit, {
    onFailure: (): A | null => null,
    onSuccess: (value): A | null => value,
  });
}

beforeEach(() => {
  clearMediaDetailCache();
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(connectedState);
});

afterEach(() => {
  rstest.restoreAllMocks();
});

test('fetchMediaDetail routes movies to item detail and caches successes', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValue({ status: 'ok', data: movieDetail });

  const first = await fetchMediaDetail('movie-1', 'Movie');
  const second = await fetchMediaDetail('movie-1', 'Movie');

  expect(Exit.isSuccess(first)).toBe(true);
  expect(Exit.isSuccess(second)).toBe(true);
  expect(itemDetail).toHaveBeenCalledTimes(1);
  expect(mediaValue(first)).toMatchObject({
    overview: 'A test movie overview.',
    genres: ['Drama', 'Sci-Fi'],
    runtimeSeconds: 7200,
    itemType: 'Movie',
  });
});

test('fetchMediaDetail routes series to show detail and nulls show-only fields', async () => {
  const showCommand = rstest
    .spyOn(commands, 'libraryShowDetail')
    .mockResolvedValue({ status: 'ok', data: showDetail });

  const result = await fetchMediaDetail('series-1', 'Series');

  expect(showCommand).toHaveBeenCalledWith('series-1');
  expect(Exit.isSuccess(result)).toBe(true);
  expect(mediaValue(result)).toMatchObject({
    itemType: 'Series',
    runtimeSeconds: null,
    genres: ['Crime', 'Thriller'],
    overview: 'A test show overview.',
  });
});

test('fetchMediaDetail passes failures through without caching them', async () => {
  const itemDetail = rstest
    .spyOn(commands, 'libraryItemDetail')
    .mockResolvedValueOnce({
      status: 'error',
      error: { code: 'network', message: 'detail unavailable' },
    })
    .mockResolvedValueOnce({ status: 'ok', data: movieDetail });

  const failed = await fetchMediaDetail('err-1', 'Movie');
  const ok = await fetchMediaDetail('err-1', 'Movie');

  expect(Exit.isSuccess(failed)).toBe(false);
  expect(Exit.isSuccess(ok)).toBe(true);
  expect(itemDetail).toHaveBeenCalledTimes(2);
});

test('MediaInfoContent renders overview, genres, runtime, resume, and user-data state', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => <MediaInfoContent detail={movieMediaDetail} />,
    root,
  );

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
      <MediaInfoHoverCard id="movie-1" itemType="Movie">
        <a href="/library/items/movie-1">Test Movie card</a>
      </MediaInfoHoverCard>
    ),
    root,
  );

  expect(screen.getByText('Test Movie card')).toBeInTheDocument();
  expect(itemDetail).not.toHaveBeenCalled();

  dispose();
  root.remove();
});
