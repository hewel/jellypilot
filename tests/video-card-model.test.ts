import { expect, test } from '@rstest/core';

import type { VideoLibraryItem } from '../src/bindings';
import {
  continueWatchingLabel,
  episodeCode,
  videoCardActionLabel,
  videoCardAriaLabel,
  videoCardDetailsTarget,
  videoCardIcon,
  videoCardProgress,
  videoCardSubtitle,
  videoCardTitle,
  videoCardTitleTarget,
} from '../src/components/library/videoCardModel';

function item(overrides: Partial<VideoLibraryItem>): VideoLibraryItem {
  return {
    artworkImageId: null,
    episodeNumber: null,
    favorite: false,
    id: 'item-1',
    itemType: 'Movie',
    name: 'Sample',
    overview: null,
    played: false,
    playedPercentage: null,
    productionYear: null,
    resumePositionSeconds: null,
    runtimeSeconds: null,
    seasonNumber: null,
    seriesId: null,
    seriesName: null,
    ...overrides,
  };
}

test('videoCardTitle prefixes episode titles with the series name', () => {
  expect(
    videoCardTitle(item({ itemType: 'Episode', name: 'Pilot', seriesName: 'Harbor Line' })),
  ).toBe('Harbor Line • Pilot');
  expect(videoCardTitle(item({ itemType: 'Episode', name: 'Pilot' }))).toBe('Pilot');
  expect(videoCardTitle(item({ name: 'Sample Movie' }))).toBe('Sample Movie');
});

test('episodeCode formats season and episode numbers', () => {
  expect(episodeCode(item({ itemType: 'Episode', seasonNumber: 2, episodeNumber: 5 }))).toBe(
    'S2 E5',
  );
  expect(episodeCode(item({ itemType: 'Episode', seasonNumber: 2 }))).toBe('Episode');
});

test('videoCardSubtitle derives Continue Watching labels with episode codes', () => {
  const episode = item({
    episodeNumber: 4,
    itemType: 'Episode',
    name: 'A Quiet Return',
    playedPercentage: 25,
    resumePositionSeconds: 120,
    runtimeSeconds: 3600,
    seasonNumber: 1,
    seriesId: 'series-1',
    seriesName: 'Silent Echoes',
  });
  expect(videoCardSubtitle(episode, { kind: 'homeRow', rowKind: 'continueWatching' })).toBe(
    '58 mins remaining · S1 E4',
  );
  expect(
    videoCardSubtitle(item({ playedPercentage: 25, runtimeSeconds: 3600 }), {
      kind: 'homeRow',
      rowKind: 'continueWatching',
    }),
  ).toBe('25% watched');
});

test('videoCardSubtitle derives row-specific metadata', () => {
  const movie = item({ productionYear: 2024 });
  expect(videoCardSubtitle(movie, { kind: 'homeRow', rowKind: 'latestMovies' })).toBe('2024');
  expect(videoCardSubtitle(item({}), { kind: 'homeRow', rowKind: 'latestMovies' })).toBeNull();

  const episode = item({ itemType: 'Episode', seasonNumber: 1, episodeNumber: 2 });
  expect(videoCardSubtitle(episode, { kind: 'homeRow', rowKind: 'nextUp' })).toBe('S1 E2');
  expect(videoCardSubtitle(episode, { kind: 'homeRow', rowKind: 'latestEpisodes' })).toBe('S1 E2');
  expect(videoCardSubtitle(movie, { kind: 'homeRow', rowKind: 'nextUp' })).toBeNull();
});

test('videoCardSubtitle prefers the production year in browse context', () => {
  expect(videoCardSubtitle(item({ productionYear: 2024 }), { kind: 'browse' })).toBe('2024');
  expect(videoCardSubtitle(item({ productionYear: 0 }), { kind: 'browse' })).toBe('0');
  expect(videoCardSubtitle(item({ itemType: 'Series' }), { kind: 'browse' })).toBe('Series');
});

test('videoCardProgress clamps percentages and derives ratios', () => {
  expect(videoCardProgress(item({ playedPercentage: 25 }))).toBe(25);
  expect(videoCardProgress(item({ playedPercentage: 140 }))).toBe(100);
  expect(videoCardProgress(item({ playedPercentage: -5 }))).toBe(0);
  expect(videoCardProgress(item({ resumePositionSeconds: 900, runtimeSeconds: 3600 }))).toBe(25);
  expect(videoCardProgress(item({ resumePositionSeconds: 7200, runtimeSeconds: 3600 }))).toBe(100);
  expect(videoCardProgress(item({ resumePositionSeconds: 900 }))).toBeNull();
  expect(videoCardProgress(item({}))).toBeNull();
});

test('continueWatchingLabel picks remaining time, watched percent, then mode', () => {
  expect(continueWatchingLabel(item({ resumePositionSeconds: 3540, runtimeSeconds: 3600 }))).toBe(
    '1 min remaining',
  );
  expect(continueWatchingLabel(item({ playedPercentage: 40 }))).toBe('40% watched');
  expect(continueWatchingLabel(item({ resumePositionSeconds: 60, runtimeSeconds: null }))).toBe(
    'Resume',
  );
  expect(continueWatchingLabel(item({}))).toBe('Play');
});

test('videoCardActionLabel reflects busy and playback decision', () => {
  const resumable = item({ name: 'Film', resumePositionSeconds: 60, runtimeSeconds: 3600 });
  expect(videoCardActionLabel(resumable, true)).toBe('Starting Film');
  expect(videoCardActionLabel(resumable, false)).toBe('Resume Film');
  expect(videoCardActionLabel(item({ name: 'Film' }), false)).toBe('Play Film');
});

test('videoCardAriaLabel notes favorites', () => {
  expect(videoCardAriaLabel(item({ name: 'Film' }))).toBe('Open Film');
  expect(videoCardAriaLabel(item({ name: 'Film', favorite: true }))).toBe('Open Film, favorite');
});

test('videoCardDetailsTarget routes Series to the show page and everything else to the item page', () => {
  expect(videoCardDetailsTarget(item({ itemType: 'Series', id: 'series-1' }))).toEqual({
    to: '/library/shows/$seriesId',
    params: { seriesId: 'series-1' },
  });
  expect(
    videoCardDetailsTarget(item({ itemType: 'Episode', id: 'episode-1', seriesId: 'series-1' })),
  ).toEqual({
    to: '/library/items/$itemId',
    params: { itemId: 'episode-1' },
  });
  expect(videoCardDetailsTarget(item({ id: 'movie-1' }))).toEqual({
    to: '/library/items/$itemId',
    params: { itemId: 'movie-1' },
  });
});

test('videoCardTitleTarget prefers series context for Series and Episodes', () => {
  expect(videoCardTitleTarget(item({ itemType: 'Series', id: 'series-1' }))).toEqual({
    to: '/library/shows/$seriesId',
    params: { seriesId: 'series-1' },
  });
  expect(
    videoCardTitleTarget(item({ itemType: 'Episode', id: 'episode-1', seriesId: 'series-1' })),
  ).toEqual({
    to: '/library/shows/$seriesId',
    params: { seriesId: 'series-1' },
  });
  expect(videoCardTitleTarget(item({ itemType: 'Episode', id: 'episode-1' }))).toEqual({
    to: '/library/items/$itemId',
    params: { itemId: 'episode-1' },
  });
  expect(videoCardTitleTarget(item({ id: 'movie-1' }))).toEqual({
    to: '/library/items/$itemId',
    params: { itemId: 'movie-1' },
  });
});

test('videoCardIcon picks the TV icon for episodic content and tv collections', () => {
  expect(videoCardIcon(item({ itemType: 'Series' }))).toBe('tv');
  expect(videoCardIcon(item({ itemType: 'Episode' }))).toBe('tv');
  expect(videoCardIcon(item({ itemType: 'Movie' }))).toBe('film');
  expect(videoCardIcon(item({ itemType: 'Movie' }), 'tvshows')).toBe('tv');
});
