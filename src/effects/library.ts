import { commands } from '@bindings';
import type {
  VideoHome,
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPage,
  VideoLibraryPlayedFilter,
  VideoLibraryPlayRequest,
  VideoLibraryShortcut,
  VideoLibrarySort,
  VideoSeasonEpisodes,
  VideoSeasonEpisodesRequest,
  VideoSeason,
  VideoShowDetail,
  VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
} from '@bindings';
import type { Exit } from 'effect';
import { Effect, Option } from 'effect';

import { runTauriCommand } from './commands';
import { connection } from './connection';
import { CommandError } from './errors';

export type LibraryExit<T> = Exit.Exit<T, CommandError>;
export type LibraryEffect<T> = Effect.Effect<T, CommandError>;

export type LibraryHomeState = VideoHome;
export type LibraryShortcutsState = VideoLibraryShortcut[];

export interface LibraryBrowseState {
  page: VideoLibraryPage;
  items: VideoLibraryItem[];
}

export type LibraryDetailState = VideoItemDetail;
export type LibraryShowState = VideoShowDetail;

export interface SeasonEpisodesState {
  page: VideoSeasonEpisodes;
}

export const LIBRARY_BROWSE_PAGE_SIZE = 24;

const disconnectedError = () =>
  new CommandError({
    message: 'Library requires a live Jellyfin connection',
  });

const requireConnection = connection.pipe(
  Effect.filterOrFail(({ connected }) => connected, disconnectedError),
);

function withConnection<T>(effect: Effect.Effect<T, CommandError>): Effect.Effect<T, CommandError> {
  return requireConnection.pipe(Effect.flatMap(() => effect));
}

export const fetchLibraryHome: LibraryEffect<LibraryHomeState> = withConnection(
  runTauriCommand(() => commands.libraryVideoHome()),
);

export const fetchLibraryShortcuts: LibraryEffect<LibraryShortcutsState> = withConnection(
  runTauriCommand(() => commands.libraryVideoShortcuts()),
);

export function fetchVideoLibraryPage(
  collectionType: VideoLibraryKind,
  libraryId: string,
  startIndex: number,
  sort: VideoLibrarySort,
  playedFilter: VideoLibraryPlayedFilter,
  favoritesOnly: boolean,
): LibraryEffect<LibraryBrowseState> {
  return runTauriCommand(() =>
    commands.libraryBrowseVideo({
      collectionType,
      favoritesOnly,
      libraryId,
      limit: LIBRARY_BROWSE_PAGE_SIZE,
      playedFilter,
      sort,
      startIndex,
    }),
  ).pipe(Effect.map((page) => ({ items: page.items, page })));
}

export function fetchVideoItemDetail(itemId: string): LibraryEffect<LibraryDetailState> {
  return withConnection(runTauriCommand(() => commands.libraryItemDetail(itemId)));
}

export function fetchVideoShowDetail(seriesId: string): LibraryEffect<LibraryShowState> {
  return withConnection(runTauriCommand(() => commands.libraryShowDetail(seriesId)));
}

export function fetchSeasonEpisodes(
  request: VideoSeasonEpisodesRequest,
): LibraryEffect<SeasonEpisodesState> {
  return withConnection(
    runTauriCommand(() => commands.librarySeasonEpisodes(request)).pipe(
      Effect.map((page) => ({ page })),
    ),
  );
}

export function startLibraryPlayback(request: VideoLibraryPlayRequest): LibraryEffect<void> {
  return withConnection(runTauriCommand(() => commands.libraryPlay(request)).pipe(Effect.asVoid));
}

export function updateLibraryUserData(
  request: VideoUserDataUpdateRequest,
): LibraryEffect<VideoUserDataUpdate> {
  return withConnection(runTauriCommand(() => commands.libraryUpdateUserData(request)));
}

/**
 * Normalized media detail backing the Media info hover-card. Unifies the
 * playable (Movie/Episode) detail and the Show detail so the hover-card renders
 * one shape regardless of item type.
 */
export interface MediaDetail {
  id: string;
  name: string;
  itemType: string;
  overview: Option.Option<string>;
  productionYear: Option.Option<number>;
  runtimeSeconds: Option.Option<number>;
  genres: string[];
  played: boolean;
  favorite: boolean;
  playedPercentage: Option.Option<number>;
  resumePositionSeconds: Option.Option<number>;
  artworkImageId: Option.Option<string>;
}

function toMediaDetail(detail: VideoItemDetail | VideoShowDetail, itemType: string): MediaDetail {
  return {
    artworkImageId: Option.fromNullishOr(detail.artworkImageId),
    favorite: detail.favorite,
    genres: detail.genres,
    id: detail.id,
    itemType,
    name: detail.name,
    overview: Option.fromNullishOr(detail.overview),
    played: detail.played,
    playedPercentage: Option.fromNullishOr(
      'playedPercentage' in detail ? detail.playedPercentage : null,
    ),
    productionYear: Option.fromNullishOr(detail.productionYear),
    resumePositionSeconds: Option.fromNullishOr(
      'resumePositionSeconds' in detail ? detail.resumePositionSeconds : null,
    ),
    runtimeSeconds: Option.fromNullishOr('runtimeSeconds' in detail ? detail.runtimeSeconds : null),
  };
}

export function fetchMediaDetail(id: string, itemType: string): LibraryEffect<MediaDetail> {
  if (itemType === 'Series') {
    return fetchVideoShowDetail(id).pipe(Effect.map((value) => toMediaDetail(value, itemType)));
  }

  return fetchVideoItemDetail(id).pipe(Effect.map((value) => toMediaDetail(value, itemType)));
}

export function initialSeasonForShow(show: LibraryShowState): Option.Option<VideoSeason> {
  return Option.match(Option.fromNullishOr(show.nextEpisode?.seasonNumber), {
    onNone: () => Option.fromNullishOr(show.seasons[0]),
    onSome: (nextSeasonNumber) =>
      Option.match(
        Option.fromNullishOr(
          show.seasons.find((season) => season.seasonNumber === nextSeasonNumber),
        ),
        {
          onNone: () => Option.fromNullishOr(show.seasons[0]),
          onSome: (season) => Option.some(season),
        },
      ),
  });
}
