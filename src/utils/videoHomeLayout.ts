import type { VideoHomeItem } from '@bindings';
import { Match } from 'effect';

export type VideoHomeRowKind = 'continueWatching' | 'nextUp' | 'latestMovies' | 'latestEpisodes';

export type VideoHomeAspect = 'poster' | 'video';
export type VideoHomeColumnCount = 1 | 2 | 3 | 4 | 5 | 6 | 7;

function finiteNumber(value: number | null): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

export const videoHomeAspect = Match.type<VideoHomeRowKind>().pipe(
  Match.when('latestMovies', (): VideoHomeAspect => 'poster'),
  Match.orElse((): VideoHomeAspect => 'video'),
);

export function videoHomeColumnCount(
  aspect: VideoHomeAspect,
  availableWidth: number,
): VideoHomeColumnCount {
  if (!Number.isFinite(availableWidth) || availableWidth <= 0) {
    return aspect === 'poster' ? 2 : 1;
  }

  if (aspect === 'video') {
    if (availableWidth >= 1380) {
      return 5;
    }
    if (availableWidth >= 1120) {
      return 4;
    }
    if (availableWidth >= 820) {
      return 3;
    }
    return availableWidth >= 560 ? 2 : 1;
  }

  if (availableWidth >= 1390) {
    return 7;
  }
  if (availableWidth >= 1160) {
    return 6;
  }
  if (availableWidth >= 950) {
    return 5;
  }
  if (availableWidth >= 700) {
    return 4;
  }
  return availableWidth >= 560 ? 3 : 2;
}

export function isValidVideoHomeResumePosition(
  resumePositionSeconds: number | null,
  runtimeSeconds: number | null,
): resumePositionSeconds is number {
  if (!finiteNumber(resumePositionSeconds) || resumePositionSeconds <= 0) {
    return false;
  }

  return (
    !finiteNumber(runtimeSeconds) || runtimeSeconds <= 0 || resumePositionSeconds < runtimeSeconds
  );
}

export type VideoHomePlaybackDecision =
  | { readonly mode: 'resume'; readonly startPositionSeconds: number }
  | { readonly mode: 'start'; readonly startPositionSeconds: null };

/**
 * Single Resume-or-Start authority for Home rows and the featured hero. Any
 * finite positive offset strictly inside a positive runtime (or with an
 * unknown runtime) resumes; every other shape starts from zero. `played` and
 * `playedPercentage` never imply an offset.
 */
export function videoHomePlaybackDecision(
  item: Pick<VideoHomeItem, 'resumePositionSeconds' | 'runtimeSeconds'>,
): VideoHomePlaybackDecision {
  if (isValidVideoHomeResumePosition(item.resumePositionSeconds, item.runtimeSeconds)) {
    return { mode: 'resume', startPositionSeconds: item.resumePositionSeconds };
  }
  return { mode: 'start', startPositionSeconds: null };
}
