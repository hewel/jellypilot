import { expect, test } from '@rstest/core';

import type { VideoLibraryItem } from '../src/bindings';
import { detailPlaybackProgress, neighboringEpisodes } from '../src/utils/libraryDetail';

function episode(id: string): VideoLibraryItem {
  return {
    id,
    name: id,
    itemType: 'Episode',
    productionYear: null,
    runtimeSeconds: null,
    played: false,
    favorite: false,
    artworkImageId: null,
    seasonNumber: 1,
    episodeNumber: null,
    seriesId: 'series-1',
    seriesName: 'Show',
    resumePositionSeconds: null,
    playedPercentage: null,
    overview: null,
  };
}

test('detailPlaybackProgress derives remaining minutes from resume position', () => {
  // 120-minute runtime, 25% in (30 minutes), 90 minutes remain.
  const progress = detailPlaybackProgress(7200, 1800, 25);
  expect(progress).not.toBeNull();
  expect(progress?.percent).toBe(25);
  expect(progress?.minutesRemaining).toBe(90);
});

test('detailPlaybackProgress prefers an explicit finite percentage', () => {
  // No resume position: percentage drives both percent and remaining time.
  const progress = detailPlaybackProgress(7200, null, 50);
  expect(progress).not.toBeNull();
  expect(progress?.percent).toBe(50);
  expect(progress?.minutesRemaining).toBe(60);
});

test('detailPlaybackProgress falls back to resume-derived percent without a percentage', () => {
  const progress = detailPlaybackProgress(7200, 1800, null);
  expect(progress).not.toBeNull();
  expect(progress?.percent).toBe(25);
  expect(progress?.minutesRemaining).toBe(90);
});

test('detailPlaybackProgress clamps remaining minutes to at least one', () => {
  // 99% through a short item rounds remaining to zero seconds -> clamped to 1.
  const progress = detailPlaybackProgress(60, null, 99);
  expect(progress).not.toBeNull();
  expect(progress?.minutesRemaining).toBe(1);
});

test('detailPlaybackProgress returns null for missing or non-positive runtime', () => {
  expect(detailPlaybackProgress(null, 1800, 25)).toBeNull();
  expect(detailPlaybackProgress(0, 1800, 25)).toBeNull();
  expect(detailPlaybackProgress(-100, 1800, 25)).toBeNull();
});

test('detailPlaybackProgress returns null for unstarted playback', () => {
  expect(detailPlaybackProgress(7200, 0, 0)).toBeNull();
  expect(detailPlaybackProgress(7200, null, null)).toBeNull();
});

test('detailPlaybackProgress returns null for fully played or out-of-range progress', () => {
  expect(detailPlaybackProgress(7200, 7200, 100)).toBeNull();
  expect(detailPlaybackProgress(7200, null, 100)).toBeNull();
  expect(detailPlaybackProgress(7200, null, 150)).toBeNull();
});

test('detailPlaybackProgress returns null for non-finite inputs', () => {
  expect(detailPlaybackProgress(Number.NaN, 1800, 25)).toBeNull();
  expect(detailPlaybackProgress(7200, Number.POSITIVE_INFINITY, 25)).toBeNull();
  expect(detailPlaybackProgress(7200, 1800, Number.NaN)).toBeNull();
});

test('neighboringEpisodes returns two before and two after in server order', () => {
  const episodes = ['e1', 'e2', 'e3', 'e4', 'e5', 'e6'].map(episode);
  const neighbors = neighboringEpisodes(episodes, 'e4').map((item) => item.id);
  expect(neighbors).toEqual(['e2', 'e3', 'e5', 'e6']);
});

test('neighboringEpisodes clamps at the season start and excludes the current item', () => {
  const episodes = ['e1', 'e2', 'e3', 'e4'].map(episode);
  expect(neighboringEpisodes(episodes, 'e1').map((item) => item.id)).toEqual(['e2', 'e3']);
  expect(neighboringEpisodes(episodes, 'e4').map((item) => item.id)).toEqual(['e2', 'e3']);
});

test('neighboringEpisodes returns empty when the current item is absent', () => {
  const episodes = ['e1', 'e2', 'e3'].map(episode);
  expect(neighboringEpisodes(episodes, 'missing')).toEqual([]);
  expect(neighboringEpisodes([], 'e1')).toEqual([]);
});
