import type {
  ConnectionState,
  MediaServerProvider,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
} from '@bindings';
import { Effect, Exit } from 'effect';

export function runExit<A, E>(effect: Effect.Effect<A, E>): Promise<Exit.Exit<A, E>> {
  return Effect.runPromiseExit(effect);
}

export type LibrarySessionKey = Readonly<{
  provider: MediaServerProvider | 'disconnected';
  serverUrl: string | null;
  userId: string | null;
}>;

const disconnectedLibrarySessionKey: LibrarySessionKey = {
  provider: 'disconnected',
  serverUrl: null,
  userId: null,
};

export function librarySessionKey(connectionState: ConnectionState | null | undefined) {
  if (!connectionState?.connected || !connectionState.serverUrl || !connectionState.userId) {
    return disconnectedLibrarySessionKey;
  }

  return {
    provider: connectionState.provider,
    serverUrl: connectionState.serverUrl,
    userId: connectionState.userId,
  } satisfies LibrarySessionKey;
}

export function librarySessionKeyFromConnectionExit(
  connectionState: Exit.Exit<ConnectionState, unknown> | undefined,
) {
  return connectionState && Exit.isSuccess(connectionState)
    ? librarySessionKey(connectionState.value)
    : disconnectedLibrarySessionKey;
}

export function isLibrarySessionKeyConnected(sessionKey: LibrarySessionKey) {
  return (
    sessionKey.provider !== 'disconnected' &&
    sessionKey.serverUrl !== null &&
    sessionKey.userId !== null
  );
}

export const queryKeys = {
  appVersion: ['app', 'version'] as const,
  appConfig: ['config', 'app'] as const,
  connectionState: ['connection', 'state'] as const,
  savedServiceProfiles: ['connection', 'profiles'] as const,
  nowPlayingState: ['nowPlaying', 'state'] as const,
  mpvTracks: (connected: boolean) => ['mpv', 'tracks', connected] as const,
  libraryRoot: ['library'] as const,
  librarySessionRoot: (sessionKey: LibrarySessionKey) => ['library', sessionKey] as const,
  libraryShortcuts: (sessionKey: LibrarySessionKey) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'shortcuts'] as const,
  libraryHome: (sessionKey: LibrarySessionKey) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'home'] as const,
  libraryBrowseRoot: (sessionKey: LibrarySessionKey) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'browse'] as const,
  libraryBrowse: (
    sessionKey: LibrarySessionKey,
    collectionType: VideoLibraryKind,
    libraryId: string,
    sort: VideoLibrarySort,
    playedFilter: VideoLibraryPlayedFilter,
    favoritesOnly: boolean,
    sortDirection: 'asc' | 'desc',
  ) =>
    [
      ...queryKeys.libraryBrowseRoot(sessionKey),
      collectionType,
      libraryId,
      sort,
      playedFilter,
      favoritesOnly,
      sortDirection,
    ] as const,
  libraryBrowsePage: (
    sessionKey: LibrarySessionKey,
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
        sessionKey,
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
  libraryItemDetail: (sessionKey: LibrarySessionKey, itemId: string) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'itemDetail', itemId] as const,
  libraryItemShortcut: (sessionKey: LibrarySessionKey, itemId: string) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'itemShortcut', itemId] as const,
  libraryShowDetail: (sessionKey: LibrarySessionKey, seriesId: string) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'showDetail', seriesId] as const,
  librarySeasonEpisodes: (sessionKey: LibrarySessionKey, seriesId: string, seasonId: string) =>
    [...queryKeys.librarySeasonEpisodesRoot(sessionKey, seriesId), seasonId] as const,
  librarySeasonEpisodesRoot: (sessionKey: LibrarySessionKey, seriesId: string) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'seasonEpisodes', seriesId] as const,
  libraryMediaDetail: (sessionKey: LibrarySessionKey, itemType: string, itemId: string) =>
    [...queryKeys.librarySessionRoot(sessionKey), 'mediaDetail', itemType, itemId] as const,
};
