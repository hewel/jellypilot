import type { VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibrarySort } from '@bindings';
import type { Exit } from 'effect';
import { Effect } from 'effect';

export function runExit<A, E>(effect: Effect.Effect<A, E>): Promise<Exit.Exit<A, E>> {
  return Effect.runPromiseExit(effect);
}

export const queryKeys = {
  appVersion: ['app', 'version'] as const,
  appConfig: ['config', 'app'] as const,
  connectionState: ['connection', 'state'] as const,
  nowPlayingState: ['nowPlaying', 'state'] as const,
  mpvTracks: (connected: boolean) => ['mpv', 'tracks', connected] as const,
  libraryShortcuts: ['library', 'shortcuts'] as const,
  libraryHome: ['library', 'home'] as const,
  libraryBrowseRoot: ['library', 'browse'] as const,
  libraryBrowse: (
    collectionType: VideoLibraryKind,
    libraryId: string,
    sort: VideoLibrarySort,
    playedFilter: VideoLibraryPlayedFilter,
    favoritesOnly: boolean,
    sortDirection: 'asc' | 'desc',
  ) =>
    [
      'library',
      'browse',
      collectionType,
      libraryId,
      sort,
      playedFilter,
      favoritesOnly,
      sortDirection,
    ] as const,
  libraryBrowsePage: (
    collectionType: VideoLibraryKind,
    libraryId: string,
    sort: VideoLibrarySort,
    playedFilter: VideoLibraryPlayedFilter,
    favoritesOnly: boolean,
    sortDirection: 'asc' | 'desc',
    startIndex: number,
  ) =>
    [
      ...queryKeys.libraryBrowse(
        collectionType,
        libraryId,
        sort,
        playedFilter,
        favoritesOnly,
        sortDirection,
      ),
      'page',
      startIndex,
    ] as const,
  libraryItemDetail: (itemId: string) => ['library', 'itemDetail', itemId] as const,
  libraryShowDetail: (seriesId: string) => ['library', 'showDetail', seriesId] as const,
  librarySeasonEpisodes: (seriesId: string, seasonId: string) =>
    ['library', 'seasonEpisodes', seriesId, seasonId] as const,
  librarySeasonEpisodesRoot: (seriesId: string) => ['library', 'seasonEpisodes', seriesId] as const,
  libraryMediaDetail: (itemType: string, itemId: string) =>
    ['library', 'mediaDetail', itemType, itemId] as const,
};
