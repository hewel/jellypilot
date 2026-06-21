import { expect, test } from '@rstest/core';

import type { VideoShowDetail } from '../src/bindings';
import { initialSeasonForShow } from '../src/effects/library';

test('initialSeasonForShow returns season matching nextEpisode.seasonNumber', () => {
  const show: VideoShowDetail = {
    id: 'show-1',
    name: 'Show 1',
    favorite: false,
    genres: [],
    overview: null,
    played: false,
    productionYear: null,
    canPlay: true,
    artworkUrl: null,
    nextEpisode: {
      id: 'ep-2',
      name: 'Episode 2',
      itemType: 'Episode',
      productionYear: null,
      runtimeSeconds: null,
      played: false,
      favorite: false,
      artworkUrl: null,
      seasonNumber: 2,
      episodeNumber: 1,
      seriesId: 'show-1',
      seriesName: 'Show 1',
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
        favorite: false,
        artworkUrl: null,
      },
    ],
  };

  const result = initialSeasonForShow(show);
  expect(result).toEqual({
    id: 'season-2',
    name: 'Season 2',
    seasonNumber: 2,
    played: false,
    favorite: false,
    artworkUrl: null,
  });
});

test('initialSeasonForShow returns first season if no matching nextEpisode.seasonNumber', () => {
  const show: VideoShowDetail = {
    id: 'show-1',
    name: 'Show 1',
    favorite: false,
    genres: [],
    overview: null,
    played: false,
    productionYear: null,
    canPlay: true,
    artworkUrl: null,
    nextEpisode: null,
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
        favorite: false,
        artworkUrl: null,
      },
    ],
  };

  const result = initialSeasonForShow(show);
  expect(result).toEqual({
    id: 'season-1',
    name: 'Season 1',
    seasonNumber: 1,
    played: false,
    favorite: false,
    artworkUrl: null,
  });
});

test('initialSeasonForShow returns null if show has no seasons', () => {
  const show: VideoShowDetail = {
    id: 'show-1',
    name: 'Show 1',
    favorite: false,
    genres: [],
    overview: null,
    played: false,
    productionYear: null,
    canPlay: true,
    artworkUrl: null,
    nextEpisode: null,
    seasons: [],
  };

  const result = initialSeasonForShow(show);
  expect(result).toBeNull();
});
