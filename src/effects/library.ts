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
import { Effect, Exit } from 'effect';

import { runTauriCommand } from './commands';
import { connection } from './connection';
import { CommandError } from './errors';

export type LibraryExit<T> = Exit.Exit<T, CommandError>;

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

export function fetchLibraryHome(): Promise<LibraryExit<LibraryHomeState>> {
  return withConnection(runTauriCommand(() => commands.libraryVideoHome())).pipe(
    Effect.runPromiseExit,
  );
}

export function fetchLibraryShortcuts(): Promise<LibraryExit<LibraryShortcutsState>> {
  return withConnection(runTauriCommand(() => commands.libraryVideoShortcuts())).pipe(
    Effect.runPromiseExit,
  );
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
        favoritesOnly,
        libraryId,
        limit: LIBRARY_BROWSE_PAGE_SIZE,
        playedFilter,
        sort,
        startIndex,
      }),
    ).pipe(Effect.map((page) => ({ items: page.items, page }))),
  ).pipe(Effect.runPromiseExit);
}

export function fetchVideoItemDetail(itemId: string): Promise<LibraryExit<LibraryDetailState>> {
  return withConnection(runTauriCommand(() => commands.libraryItemDetail(itemId))).pipe(
    Effect.runPromiseExit,
  );
}

export function fetchVideoShowDetail(seriesId: string): Promise<LibraryExit<LibraryShowState>> {
  return withConnection(runTauriCommand(() => commands.libraryShowDetail(seriesId))).pipe(
    Effect.runPromiseExit,
  );
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

export function startLibraryPlayback(request: VideoLibraryPlayRequest): Promise<LibraryExit<void>> {
  return withConnection(
    runTauriCommand(() => commands.libraryPlay(request)).pipe(Effect.asVoid),
  ).pipe(Effect.runPromiseExit);
}

export function updateLibraryUserData(
  request: VideoUserDataUpdateRequest,
): Promise<LibraryExit<VideoUserDataUpdate>> {
  return withConnection(runTauriCommand(() => commands.libraryUpdateUserData(request))).pipe(
    Effect.runPromiseExit,
  );
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

// Ponytail: session-scoped cache; no invalidation. Detail rarely changes, and
// Re-fetch on disconnect/reconnect is acceptable if staleness ever matters.
const mediaDetailCache = new Map<string, MediaDetail>();

/** Clear the hover-card detail cache. Intended for tests and later invalidation. */
export function clearMediaDetailCache(): void {
  mediaDetailCache.clear();
}

function toMediaDetail(detail: VideoItemDetail | VideoShowDetail, itemType: string): MediaDetail {
  return {
    artworkUrl: detail.artworkUrl,
    favorite: detail.favorite,
    genres: detail.genres,
    id: detail.id,
    itemType,
    name: detail.name,
    overview: detail.overview,
    played: detail.played,
    playedPercentage: 'playedPercentage' in detail ? detail.playedPercentage : null,
    productionYear: detail.productionYear,
    resumePositionSeconds: 'resumePositionSeconds' in detail ? detail.resumePositionSeconds : null,
    runtimeSeconds: 'runtimeSeconds' in detail ? detail.runtimeSeconds : null,
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
  if (cached) {
    return Exit.succeed(cached);
  }

  const exit =
    itemType === 'Series' ? await fetchVideoShowDetail(id) : await fetchVideoItemDetail(id);

  return Exit.map(exit, (detail: VideoItemDetail | VideoShowDetail) => {
    const media = toMediaDetail(detail, itemType);
    mediaDetailCache.set(id, media);
    return media;
  });
}

export function initialSeasonForShow(show: LibraryShowState): VideoSeason | null {
  const nextSeasonNumber = show.nextEpisode?.seasonNumber ?? null;
  if (nextSeasonNumber !== null) {
    const match = show.seasons.find((season) => season.seasonNumber === nextSeasonNumber);
    if (match) {
      return match;
    }
  }

  return show.seasons[0] ?? null;
}

export async function fetchInitialSeasonEpisodes(
  seriesId: string,
  show: Promise<LibraryExit<LibraryShowState>>,
): Promise<LibraryExit<SeasonEpisodesState> | null> {
  const showExit = await show;
  if (!Exit.isSuccess(showExit)) {
    return null;
  }

  const season = initialSeasonForShow(showExit.value);
  if (!season) {
    return null;
  }

  return fetchSeasonEpisodes({
    seasonId: season.id,
    seasonNumber: season.seasonNumber,
    seriesId,
  });
}
