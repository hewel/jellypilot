import type { VideoLibraryItem } from '@bindings';

export interface DetailPlaybackProgress {
  percent: number;
  minutesRemaining: number;
}

/**
 * Derive a compact playback-progress summary for the detail hero.
 *
 * Returns `null` when there is nothing meaningful to show: missing or
 * non-positive runtime, unstarted playback, fully-played items, or any
 * non-finite input. Prefers an explicit `playedPercentage` when it is finite,
 * otherwise derives the percentage from the resume position and runtime. The
 * final percent must land strictly between 0 and 100.
 */
export function detailPlaybackProgress(
  runtimeSeconds: number | null,
  resumePositionSeconds: number | null,
  playedPercentage: number | null,
): DetailPlaybackProgress | null {
  // Any non-finite input is treated as unusable data.
  if (
    (runtimeSeconds !== null && !Number.isFinite(runtimeSeconds)) ||
    (resumePositionSeconds !== null && !Number.isFinite(resumePositionSeconds)) ||
    (playedPercentage !== null && !Number.isFinite(playedPercentage))
  ) {
    return null;
  }

  if (runtimeSeconds === null || runtimeSeconds <= 0) {
    return null;
  }

  const hasResume = resumePositionSeconds !== null && resumePositionSeconds > 0;
  const hasPercentage = playedPercentage !== null;

  if (!hasResume && !hasPercentage) {
    return null;
  }

  const percent = clamp(
    hasPercentage
      ? (playedPercentage as number)
      : ((resumePositionSeconds as number) / runtimeSeconds) * 100,
  );

  if (percent <= 0 || percent >= 100) {
    return null;
  }

  const resumeSeconds = hasResume
    ? (resumePositionSeconds as number)
    : ((playedPercentage as number) / 100) * runtimeSeconds;
  const remainingSeconds = Math.max(0, runtimeSeconds - resumeSeconds);

  return {
    percent,
    minutesRemaining: Math.max(1, Math.round(remainingSeconds / 60)),
  };
}

function clamp(value: number): number {
  return Math.min(100, Math.max(0, value));
}

/**
 * Select the episodes surrounding the current item within a season, preserving
 * the server-returned order. Returns up to two entries before and two after the
 * current item, excluding the current item itself. Returns `[]` when the
 * current item is absent from the season.
 */
export function neighboringEpisodes(
  episodes: readonly VideoLibraryItem[],
  currentItemId: string,
): VideoLibraryItem[] {
  const index = episodes.findIndex((episode) => episode.id === currentItemId);
  if (index === -1) {
    return [];
  }

  const before = episodes.slice(Math.max(0, index - 2), index);
  const after = episodes.slice(index + 1, index + 3);
  return [...before, ...after];
}
