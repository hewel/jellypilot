import {
  type ConnectionState,
  commands,
  type VideoHome,
  type VideoItemDetail,
  type VideoLibraryItem,
  type VideoLibraryKind,
  type VideoLibraryPage,
  type VideoLibraryPlayedFilter,
  type VideoLibraryPlayRequest,
  type VideoLibrarySort,
  type VideoSearchPage,
  type VideoSeasonEpisodes,
  type VideoSeasonEpisodesRequest,
  type VideoShowDetail,
  type VideoUserDataUpdateRequest,
} from '@bindings';
import { Effect, Exit } from 'effect';
import { connection } from '~effects/auth';
import {
  commandFailureMessage,
  runTauriCommand,
  runTauriCommandRaw,
} from '~effects/commands';
import { CommandError } from '~effects/errors';

export type LibraryHomeState =
  | { kind: 'ready'; home: VideoHome; connection: ConnectionState }
  | { kind: 'empty'; connection: ConnectionState }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

export type LibraryBrowseState =
  | { kind: 'ready'; page: VideoLibraryPage; items: VideoLibraryItem[] }
  | { kind: 'empty'; page: VideoLibraryPage }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

export type LibrarySearchState =
  | { kind: 'ready'; page: VideoSearchPage; items: VideoLibraryItem[] }
  | { kind: 'empty'; page: VideoSearchPage }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

export type LibraryDetailState =
  | { kind: 'ready'; detail: VideoItemDetail }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

export type LibraryShowState =
  | { kind: 'ready'; detail: VideoShowDetail }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

export type SeasonEpisodesState =
  | { kind: 'ready'; page: VideoSeasonEpisodes }
  | { kind: 'empty'; page: VideoSeasonEpisodes }
  | { kind: 'error'; message: string };

export const LIBRARY_BROWSE_PAGE_SIZE = 24;
export const LIBRARY_SEARCH_PAGE_SIZE = 24;

export function videoHomeIsEmpty(home: VideoHome) {
  return (
    home.continueWatching.length === 0 &&
    home.nextUp.length === 0 &&
    home.latestMovies.length === 0 &&
    home.latestEpisodes.length === 0 &&
    home.libraryShortcuts.length === 0
  );
}

export async function fetchLibraryHome() {
  return connection
    .pipe(
      Effect.flatMap(({ connected }) =>
        connected
          ? runTauriCommand(() => commands.libraryVideoHome())
          : Effect.fail(
              new CommandError({
                message: 'disconnected',
              }),
            ),
      ),
    )
    .pipe(
      Effect.mapError((error) => ({
        kind: 'error',
        message: error.message,
      })),
    )
    .pipe(Effect.runPromiseExit);
}

export async function fetchVideoLibraryPage(
  collectionType: VideoLibraryKind,
  libraryId: string,
  startIndex: number,
  sort: VideoLibrarySort,
  playedFilter: VideoLibraryPlayedFilter,
  favoritesOnly: boolean,
): Promise<LibraryBrowseState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const page = await Effect.runPromiseExit(
    runTauriCommand(() =>
      commands.libraryBrowseVideo({
        collectionType,
        libraryId,
        startIndex,
        limit: LIBRARY_BROWSE_PAGE_SIZE,
        sort,
        playedFilter,
        favoritesOnly,
      }),
    ),
  );

  if (!Exit.isSuccess(page)) {
    return {
      kind: 'error',
      message: commandFailureMessage(page.cause, 'Could not load Library page'),
    };
  }

  return page.value.items.length === 0
    ? { kind: 'empty', page: page.value }
    : { kind: 'ready', page: page.value, items: page.value.items };
}

export async function fetchVideoSearchPage(
  query: string,
  startIndex: number,
): Promise<LibrarySearchState> {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) {
    return { kind: 'error', message: 'Search text is required' };
  }

  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const page = await Effect.runPromiseExit(
    runTauriCommand(() =>
      commands.librarySearchVideo({
        query: trimmedQuery,
        startIndex,
        limit: LIBRARY_SEARCH_PAGE_SIZE,
      }),
    ),
  );

  if (!Exit.isSuccess(page)) {
    return {
      kind: 'error',
      message: commandFailureMessage(page.cause, 'Could not search Library'),
    };
  }

  return page.value.items.length === 0
    ? { kind: 'empty', page: page.value }
    : { kind: 'ready', page: page.value, items: page.value.items };
}

export async function fetchVideoItemDetail(
  itemId: string,
): Promise<LibraryDetailState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const detail = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryItemDetail(itemId)),
  );

  if (!Exit.isSuccess(detail)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        detail.cause,
        'Could not load item detail',
      ),
    };
  }

  return { kind: 'ready', detail: detail.value };
}

export async function fetchVideoShowDetail(
  seriesId: string,
): Promise<LibraryShowState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const detail = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryShowDetail(seriesId)),
  );

  if (!Exit.isSuccess(detail)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        detail.cause,
        'Could not load show detail',
      ),
    };
  }

  return { kind: 'ready', detail: detail.value };
}

export async function fetchSeasonEpisodes(
  request: VideoSeasonEpisodesRequest,
): Promise<SeasonEpisodesState> {
  const page = await Effect.runPromiseExit(
    runTauriCommand(() => commands.librarySeasonEpisodes(request)),
  );

  if (!Exit.isSuccess(page)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        page.cause,
        'Could not load season episodes',
      ),
    };
  }

  return page.value.episodes.length === 0
    ? { kind: 'empty', page: page.value }
    : { kind: 'ready', page: page.value };
}

export async function startLibraryPlayback(
  request: VideoLibraryPlayRequest,
): Promise<string | null> {
  const result = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryPlay(request)),
  );

  if (!Exit.isSuccess(result)) {
    return commandFailureMessage(result.cause, 'Could not start playback');
  }

  return null;
}

export async function updateLibraryUserData(
  request: VideoUserDataUpdateRequest,
): Promise<string | null> {
  const result = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryUpdateUserData(request)),
  );

  if (!Exit.isSuccess(result)) {
    return commandFailureMessage(result.cause, 'Could not update user data');
  }

  return null;
}
