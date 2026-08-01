import type { VideoLibraryItem, VideoLibraryKind } from '@bindings';
import { Match } from 'effect';
import {
  isValidVideoHomeResumePosition,
  videoHomePlaybackDecision,
  type VideoHomeRowKind,
} from '~utils/videoHomeLayout';

/**
 * Single derivation authority behind every video card: titles, subtitles,
 * navigation targets, icons, progress, and action labels. Pure — no JSX, no
 * Solid imports — so renderers and tests share one source of truth.
 */
export type VideoCardContext = { kind: 'homeRow'; rowKind: VideoHomeRowKind } | { kind: 'browse' };

export function videoCardTitle(item: VideoLibraryItem): string {
  if (item.itemType === 'Episode') {
    return item.seriesName === null ? item.name : `${item.seriesName} • ${item.name}`;
  }

  return item.name;
}

export function episodeCode(item: VideoLibraryItem): string {
  return item.seasonNumber !== null && item.episodeNumber !== null
    ? `S${item.seasonNumber} E${item.episodeNumber}`
    : 'Episode';
}

const homeRowSubtitle = Match.type<{
  item: VideoLibraryItem;
  rowKind: VideoHomeRowKind;
}>().pipe(
  Match.when({ rowKind: 'continueWatching' }, ({ item }) =>
    item.itemType === 'Episode'
      ? `${continueWatchingLabel(item)} · ${episodeCode(item)}`
      : continueWatchingLabel(item),
  ),
  Match.when({ rowKind: 'latestMovies' }, ({ item }) =>
    item.productionYear === null ? null : item.productionYear.toString(),
  ),
  Match.when({ rowKind: Match.is('nextUp', 'latestEpisodes') }, ({ item }) =>
    item.itemType === 'Episode' ? episodeCode(item) : null,
  ),
  Match.exhaustive,
);

export function videoCardSubtitle(
  item: VideoLibraryItem,
  context: VideoCardContext,
): string | null {
  if (context.kind === 'browse') {
    return item.productionYear !== null ? item.productionYear.toString() : item.itemType;
  }
  return homeRowSubtitle({ item, rowKind: context.rowKind });
}

/** Artwork/"open" navigation target: the item itself, except Series → show page. */
export function videoCardDetailsTarget(item: VideoLibraryItem) {
  return item.itemType === 'Series'
    ? ({ to: '/library/shows/$seriesId', params: { seriesId: item.id } } as const)
    : ({ to: '/library/items/$itemId', params: { itemId: item.id } } as const);
}

/** Title-link target: series context when one exists, otherwise the item page. */
export function videoCardTitleTarget(item: VideoLibraryItem) {
  if (item.itemType === 'Series') {
    return { to: '/library/shows/$seriesId', params: { seriesId: item.id } } as const;
  }
  if (item.itemType === 'Episode' && item.seriesId !== null) {
    return { to: '/library/shows/$seriesId', params: { seriesId: item.seriesId } } as const;
  }
  return { to: '/library/items/$itemId', params: { itemId: item.id } } as const;
}

export function videoCardIcon(
  item: VideoLibraryItem,
  collectionType?: VideoLibraryKind,
): 'tv' | 'film' {
  return collectionType === 'tvshows' || item.itemType === 'Series' || item.itemType === 'Episode'
    ? 'tv'
    : 'film';
}

function finiteNumber(value: number | null): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

export function videoCardProgress(item: VideoLibraryItem): number | null {
  if (finiteNumber(item.playedPercentage)) {
    return Math.min(100, Math.max(0, item.playedPercentage));
  }
  if (
    finiteNumber(item.runtimeSeconds) &&
    item.runtimeSeconds > 0 &&
    finiteNumber(item.resumePositionSeconds) &&
    item.resumePositionSeconds >= 0
  ) {
    return Math.min(100, Math.max(0, (item.resumePositionSeconds / item.runtimeSeconds) * 100));
  }
  return null;
}

export function continueWatchingLabel(item: VideoLibraryItem): string {
  if (
    isValidVideoHomeResumePosition(item.resumePositionSeconds, item.runtimeSeconds) &&
    finiteNumber(item.runtimeSeconds)
  ) {
    const minutes = Math.max(1, Math.ceil((item.runtimeSeconds - item.resumePositionSeconds) / 60));
    return `${minutes} ${minutes === 1 ? 'min' : 'mins'} remaining`;
  }

  const progress = videoCardProgress(item);
  if (progress !== null) {
    return `${Math.round(progress)}% watched`;
  }
  return videoHomePlaybackDecision(item).mode === 'resume' ? 'Resume' : 'Play';
}

export function videoCardActionLabel(item: VideoLibraryItem, busy: boolean): string {
  if (busy) {
    return `Starting ${item.name}`;
  }
  return videoHomePlaybackDecision(item).mode === 'resume'
    ? `Resume ${item.name}`
    : `Play ${item.name}`;
}

export function videoCardAriaLabel(item: VideoLibraryItem): string {
  return `Open ${item.name}${item.favorite ? ', favorite' : ''}`;
}
