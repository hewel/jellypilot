import type { VideoLibraryItem } from '@bindings';
import { Link } from '@tanstack/solid-router';
import { Film, Play, RefreshCw } from 'lucide-solid';
import { For, type JSX, Show } from 'solid-js';

import { Button } from '../ui';
import * as styles from './DetailEpisodeList.styles';
import { LibraryImage } from './LibraryImage';
import { formatRuntime } from './shared';

function episodeCode(episode: VideoLibraryItem): string {
  const { seasonNumber, episodeNumber } = episode;
  if (seasonNumber !== null && episodeNumber !== null) {
    return `S${seasonNumber.toString().padStart(2, '0')}E${episodeNumber.toString().padStart(2, '0')}`;
  }
  return 'Episode';
}

function hasResume(episode: VideoLibraryItem): boolean {
  return (
    episode.resumePositionSeconds !== null && episode.resumePositionSeconds > 0 && !episode.played
  );
}

/**
 * Rich episode rows shared by the show detail season list and the item detail
 * "More from Season" shelf. Rows never start playback themselves; only the
 * labeled Play/Resume button does. Server order and user-state indicators are
 * preserved verbatim.
 */
export function DetailEpisodeList(props: {
  episodes: readonly VideoLibraryItem[];
  busyItemId: string | null;
  disabled: boolean;
  onPlay: (episode: VideoLibraryItem) => void;
}): JSX.Element {
  return (
    <div class={styles.list}>
      <For each={props.episodes}>
        {(episode) => {
          const busy = () => props.busyItemId === episode.id;
          const resumable = () => hasResume(episode);
          return (
            <div class={styles.row}>
              <div class={styles.thumb}>
                <LibraryImage
                  imageId={episode.artworkImageId}
                  alt={`${episode.name} thumbnail`}
                  class={styles.thumbImage}
                  loading="lazy"
                  fallback={
                    <div class={styles.thumbFallback} aria-hidden="true">
                      <Film class={styles.thumbIcon} />
                    </div>
                  }
                />
              </div>

              <div class={styles.copy}>
                <div class={styles.titleRow}>
                  <span class={styles.episodeCode}>{episodeCode(episode)}</span>
                  <Link
                    to="/library/items/$itemId"
                    params={{ itemId: episode.id }}
                    class={styles.title}
                  >
                    {episode.name}
                  </Link>
                </div>
                <Show when={episode.overview}>
                  {(overview) => <p class={styles.overview}>{overview()}</p>}
                </Show>
                <div class={styles.subRow}>
                  <Show when={formatRuntime(episode.runtimeSeconds)}>
                    {(runtime) => <span>{runtime()}</span>}
                  </Show>
                  <Show when={episode.played}>
                    <Show when={formatRuntime(episode.runtimeSeconds)}>
                      <span class={styles.subSeparator} aria-hidden="true">
                        ·
                      </span>
                    </Show>
                    <span class={styles.playedTag}>Played</span>
                  </Show>
                  <Show when={resumable()}>
                    <span class={styles.subSeparator} aria-hidden="true">
                      ·
                    </span>
                    <span class={styles.progressTag}>
                      {Math.round(episode.playedPercentage ?? 0)}% watched
                    </span>
                  </Show>
                </div>
              </div>

              <Button
                type="button"
                variant="outlined"
                class={styles.playButton}
                disabled={props.disabled || busy()}
                aria-label={`${resumable() ? 'Resume' : 'Play'} ${episode.name}`}
                onClick={() => props.onPlay(episode)}
                leadingIcon={
                  <Show when={busy()} fallback={<Play class={styles.playIcon} />}>
                    <RefreshCw class={styles.spinner} />
                  </Show>
                }
              >
                {busy() ? 'Loading…' : resumable() ? 'Resume' : 'Play'}
              </Button>
            </div>
          );
        }}
      </For>
    </div>
  );
}

export function DetailEpisodeListSkeleton(props: { rows?: number }): JSX.Element {
  const rows = () => props.rows ?? 3;
  return (
    <div class={styles.skeletonList} role="status" aria-label="Loading episodes">
      <For each={Array.from({ length: rows() })}>{() => <div class={styles.skeletonRow} />}</For>
    </div>
  );
}
