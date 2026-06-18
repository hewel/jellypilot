import {
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
  type VideoUserDataUpdate,
  type VideoUserDataUpdateRequest,
} from '@bindings';
import type { Exit } from 'effect';
import { Effect, String as EffectString, Option } from 'effect';
import { connection } from './auth';
import { runTauriCommand } from './commands';
import { CommandError } from './errors';

export type LibraryExit<T> = Exit.Exit<T, CommandError>;

export type LibraryHomeState = VideoHome;

export interface LibraryBrowseState {
  page: VideoLibraryPage;
  items: VideoLibraryItem[];
}

export interface LibrarySearchState {
  page: VideoSearchPage;
  items: VideoLibraryItem[];
}

export type LibraryDetailState = VideoItemDetail;
export type LibraryShowState = VideoShowDetail;

export interface SeasonEpisodesState {
  page: VideoSeasonEpisodes;
}

export const LIBRARY_BROWSE_PAGE_SIZE = 24;
export const LIBRARY_SEARCH_PAGE_SIZE = 24;

const disconnectedError = () =>
  new CommandError({
    message: 'Library requires a live Jellyfin connection',
  });

const requireConnection = connection.pipe(
  Effect.filterOrFail(({ connected }) => connected, disconnectedError),
);

function withConnection<T>(
  effect: Effect.Effect<T, CommandError>,
): Effect.Effect<T, CommandError> {
  return requireConnection.pipe(Effect.flatMap(() => effect));
}

function requiredSearchQuery(
  query: string,
): Effect.Effect<string, CommandError> {
  return Effect.fromOption(
    Option.fromNullishOr(query).pipe(
      Option.map(EffectString.trim),
      Option.filter(EffectString.isNonEmpty),
    ),
  ).pipe(
    Effect.mapError(
      () =>
        new CommandError({
          message: 'Search text is required',
        }),
    ),
  );
}

export function fetchLibraryHome(): Promise<LibraryExit<LibraryHomeState>> {
  return withConnection(
    runTauriCommand(() => commands.libraryVideoHome()),
  ).pipe(Effect.runPromiseExit);
}

export function fetchVideoLibraryPage(
  collectionType: VideoLibraryKind,
  libraryId: string,
  startIndex: number,
  sort: VideoLibrarySort,
  playedFilter: VideoLibraryPlayedFilter,
  favoritesOnly: boolean,
): Promise<LibraryExit<LibraryBrowseState>> {
  return withConnection(
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
    ).pipe(Effect.map((page) => ({ page, items: page.items }))),
  ).pipe(Effect.runPromiseExit);
}

export function fetchVideoSearchPage(
  query: string,
  startIndex: number,
): Promise<LibraryExit<LibrarySearchState>> {
  return requiredSearchQuery(query).pipe(
    Effect.flatMap((trimmedQuery) =>
      withConnection(
        runTauriCommand(() =>
          commands.librarySearchVideo({
            query: trimmedQuery,
            startIndex,
            limit: LIBRARY_SEARCH_PAGE_SIZE,
          }),
        ).pipe(Effect.map((page) => ({ page, items: page.items }))),
      ),
    ),
    Effect.runPromiseExit,
  );
}

export function fetchVideoItemDetail(
  itemId: string,
): Promise<LibraryExit<LibraryDetailState>> {
  return withConnection(
    runTauriCommand(() => commands.libraryItemDetail(itemId)),
  ).pipe(Effect.runPromiseExit);
}

export function fetchVideoShowDetail(
  seriesId: string,
): Promise<LibraryExit<LibraryShowState>> {
  return withConnection(
    runTauriCommand(() => commands.libraryShowDetail(seriesId)),
  ).pipe(Effect.runPromiseExit);
}

export function fetchSeasonEpisodes(
  request: VideoSeasonEpisodesRequest,
): Promise<LibraryExit<SeasonEpisodesState>> {
  return withConnection(
    runTauriCommand(() => commands.librarySeasonEpisodes(request)).pipe(
      Effect.map((page) => ({ page })),
    ),
  ).pipe(Effect.runPromiseExit);
}

export function startLibraryPlayback(
  request: VideoLibraryPlayRequest,
): Promise<LibraryExit<void>> {
  return withConnection(
    runTauriCommand(() => commands.libraryPlay(request)).pipe(Effect.asVoid),
  ).pipe(Effect.runPromiseExit);
}

export function updateLibraryUserData(
  request: VideoUserDataUpdateRequest,
): Promise<LibraryExit<VideoUserDataUpdate>> {
  return withConnection(
    runTauriCommand(() => commands.libraryUpdateUserData(request)),
  ).pipe(Effect.runPromiseExit);
}
