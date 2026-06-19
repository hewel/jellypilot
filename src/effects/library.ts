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
import { Effect, String as EffectString, Exit, Option } from 'effect';
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

/**
 * Normalized media detail backing the Media info hover-card. Unifies the
 * playable (Movie/Episode) detail and the Show detail so the hover-card renders
 * one shape regardless of item type.
 */
export interface MediaDetail {
  id: string;
  name: string;
  itemType: string;
  overview: string | null;
  productionYear: number | null;
  runtimeSeconds: number | null;
  genres: string[];
  played: boolean;
  favorite: boolean;
  playedPercentage: number | null;
  resumePositionSeconds: number | null;
  artworkUrl: string | null;
}

// ponytail: session-scoped cache; no invalidation. Detail rarely changes, and
// re-fetch on disconnect/reconnect is acceptable if staleness ever matters.
const mediaDetailCache = new Map<string, MediaDetail>();

/** Clear the hover-card detail cache. Intended for tests and later invalidation. */
export function clearMediaDetailCache(): void {
  mediaDetailCache.clear();
}

function toMediaDetail(
  detail: VideoItemDetail | VideoShowDetail,
  itemType: string,
): MediaDetail {
  return {
    id: detail.id,
    name: detail.name,
    itemType,
    overview: detail.overview,
    productionYear: detail.productionYear,
    runtimeSeconds: 'runtimeSeconds' in detail ? detail.runtimeSeconds : null,
    genres: detail.genres,
    played: detail.played,
    favorite: detail.favorite,
    playedPercentage:
      'playedPercentage' in detail ? detail.playedPercentage : null,
    resumePositionSeconds:
      'resumePositionSeconds' in detail ? detail.resumePositionSeconds : null,
    artworkUrl: detail.artworkUrl,
  };
}

/**
 * Fetch normalized media detail for a hover-card preview. Routes Series to the
 * show detail command and everything else to the item detail command. Successes
 * are cached per item id so repeated hovers do not re-fetch.
 */
export async function fetchMediaDetail(
  id: string,
  itemType: string,
): Promise<LibraryExit<MediaDetail>> {
  const cached = mediaDetailCache.get(id);
  if (cached) return Exit.succeed(cached);

  const exit =
    itemType === 'Series'
      ? await fetchVideoShowDetail(id)
      : await fetchVideoItemDetail(id);

  return Exit.map(exit, (detail: VideoItemDetail | VideoShowDetail) => {
    const media = toMediaDetail(detail, itemType);
    mediaDetailCache.set(id, media);
    return media;
  });
}
