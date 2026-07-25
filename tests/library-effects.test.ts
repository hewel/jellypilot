import { afterEach, expect, rstest, test } from '@rstest/core';
import { Effect, Exit, Option } from 'effect';

import { commands } from '../src/bindings';
import type {
  ConnectionState,
  VideoLibraryPage,
  VideoSearchPage,
  VideoShowDetail,
} from '../src/bindings';
import {
  fetchVideoLibraryPage,
  fetchVideoSearchPage,
  initialSeasonForShow,
} from '../src/effects/library';

const connectedState: ConnectionState = {
  capabilities: {
    introSkipper: true,
    quickConnect: true,
    remoteControl: true,
    remoteControlAvailable: true,
    remoteControlWarning: null,
  },
  connected: true,
  provider: 'jellyfin',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

const page: VideoLibraryPage = {
  collectionType: 'movies',
  hasMore: false,
  items: [],
  libraryId: 'movies',
  limit: 24,
  startIndex: 0,
  totalRecordCount: 0,
};

afterEach(() => {
  rstest.restoreAllMocks();
});

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
    artworkImageId: null,
    backdropImageId: null,
    nextEpisode: {
      id: 'ep-2',
      name: 'Episode 2',
      itemType: 'Episode',
      productionYear: null,
      runtimeSeconds: null,
      played: false,
      favorite: false,
      artworkImageId: null,
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
        artworkImageId: null,
      },
      {
        id: 'season-2',
        name: 'Season 2',
        seasonNumber: 2,
        played: false,
        favorite: false,
        artworkImageId: null,
      },
    ],
  };

  const result = initialSeasonForShow(show);
  expect(Option.isSome(result)).toBe(true);
  if (Option.isSome(result)) {
    expect(result.value).toEqual({
      id: 'season-2',
      name: 'Season 2',
      seasonNumber: 2,
      played: false,
      favorite: false,
      artworkImageId: null,
    });
  }
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
    artworkImageId: null,
    backdropImageId: null,
    nextEpisode: null,
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
        favorite: false,
        artworkImageId: null,
      },
    ],
  };

  const result = initialSeasonForShow(show);
  expect(Option.isSome(result)).toBe(true);
  if (Option.isSome(result)) {
    expect(result.value).toEqual({
      id: 'season-1',
      name: 'Season 1',
      seasonNumber: 1,
      played: false,
      favorite: false,
      artworkImageId: null,
    });
  }
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
    artworkImageId: null,
    backdropImageId: null,
    nextEpisode: null,
    seasons: [],
  };

  const result = initialSeasonForShow(show);
  expect(Option.isNone(result)).toBe(true);
});

test('fetchVideoSearchPage sends the paged query without a connection preflight', async () => {
  const serverGetState = rstest.spyOn(commands, 'serverGetState').mockResolvedValue(connectedState);
  const searchPage: VideoSearchPage = {
    query: 'alien',
    startIndex: 0,
    limit: 24,
    totalRecordCount: 0,
    hasMore: false,
    items: [],
  };
  const search = rstest.spyOn(commands, 'librarySearchVideo').mockResolvedValue({
    data: searchPage,
    status: 'ok',
  });

  const exit = await Effect.runPromiseExit(fetchVideoSearchPage('alien', 0));

  expect(Exit.isSuccess(exit)).toBe(true);
  expect(serverGetState).not.toHaveBeenCalled();
  expect(search).toHaveBeenCalledWith({ query: 'alien', startIndex: 0, limit: 24 });
});

test('fetchVideoLibraryPage does not preflight connection state per page', async () => {
  const serverGetState = rstest.spyOn(commands, 'serverGetState').mockResolvedValue(connectedState);
  const browse = rstest.spyOn(commands, 'libraryBrowseVideo').mockResolvedValue({
    data: page,
    status: 'ok',
  });

  const exit = await Effect.runPromiseExit(
    fetchVideoLibraryPage('movies', 'movies', 0, 'title', 'all', false),
  );

  expect(Exit.isSuccess(exit)).toBe(true);
  expect(serverGetState).not.toHaveBeenCalled();
  expect(browse).toHaveBeenCalledWith({
    collectionType: 'movies',
    favoritesOnly: false,
    libraryId: 'movies',
    limit: 24,
    playedFilter: 'all',
    sort: 'title',
    startIndex: 0,
  });
});
