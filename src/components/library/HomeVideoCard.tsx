import type { VideoHomeItem } from '@bindings';
import { cx } from '@styled-system/css';
import { Link } from '@tanstack/solid-router';
import { Match } from 'effect';
import { Film, LoaderCircle, Play, Tv } from 'lucide-solid';
import { Show, createEffect, createMemo } from 'solid-js';
import { imageSource } from '~utils/imageSource';
import {
  isValidVideoHomeResumePosition,
  videoHomeAspect,
  type VideoHomeRowKind,
} from '~utils/videoHomeLayout';

import { LibraryImage } from './LibraryImage';
import * as styles from './VideoCard.styles';
import { CardTitle, type VideoCardAspectClass } from './videoCardShared';

export interface HomeVideoCardProps {
  item: VideoHomeItem;
  rowKind: VideoHomeRowKind;
  busy?: boolean;
  resumeDisabled?: boolean;
  onResume?: () => void;
}

const homeSecondary = Match.type<{
  item: VideoHomeItem;
  rowKind: VideoHomeRowKind;
}>().pipe(
  Match.when({ rowKind: 'continueWatching' }, ({ item }) => continueWatchingLabel(item)),
  Match.when({ rowKind: 'latestMovies' }, ({ item }) =>
    item.productionYear === null ? null : item.productionYear.toString(),
  ),
  Match.when({ rowKind: Match.is('nextUp', 'latestEpisodes') }, ({ item }) =>
    item.seriesName === null ? null : item.name,
  ),
  Match.exhaustive,
);

function finiteNumber(value: number | null): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

export function videoHomeProgress(item: VideoHomeItem): number | null {
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

export function continueWatchingLabel(item: VideoHomeItem): string {
  if (
    finiteNumber(item.runtimeSeconds) &&
    finiteNumber(item.resumePositionSeconds) &&
    item.runtimeSeconds > item.resumePositionSeconds
  ) {
    const minutes = Math.max(1, Math.ceil((item.runtimeSeconds - item.resumePositionSeconds) / 60));
    return `${minutes} ${minutes === 1 ? 'min' : 'mins'} remaining`;
  }

  const progress = videoHomeProgress(item);
  return progress === null ? 'Resume' : `${Math.round(progress)}% watched`;
}

function homeTitle(item: VideoHomeItem): string {
  if (item.itemType === 'Episode') {
    const identity = item.seriesName ?? item.name;
    const episode =
      item.seasonNumber !== null && item.episodeNumber !== null
        ? `S${item.seasonNumber} E${item.episodeNumber}`
        : 'Episode';
    return `${identity} • ${episode}`;
  }

  return `${item.name} • ${item.itemType}`;
}

function isDirectResumeCard(props: HomeVideoCardProps): props is HomeVideoCardProps & {
  onResume: () => void;
} {
  return (
    props.rowKind === 'continueWatching' &&
    props.onResume !== undefined &&
    isValidVideoHomeResumePosition(props.item.resumePositionSeconds, props.item.runtimeSeconds)
  );
}

export function HomeVideoCard(props: HomeVideoCardProps) {
  const aspectClass = (): VideoCardAspectClass => videoHomeAspect(props.rowKind);

  const artworkImageId = () => props.item.artworkImageId;
  const artworkUrl = createMemo(() => {
    const imageId = artworkImageId();
    return imageId ? imageSource(imageId) : '';
  });
  const showPlayBadge = () => isDirectResumeCard(props) && !props.busy;

  createEffect(() => {
    artworkUrl();
  });

  const usesTvIcon = () => props.item.itemType === 'Series' || props.item.itemType === 'Episode';

  const progress = () =>
    props.rowKind === 'continueWatching' ? videoHomeProgress(props.item) : null;
  const secondary = () => homeSecondary({ item: props.item, rowKind: props.rowKind });

  const titleLinkTarget = () => {
    if (props.item.itemType === 'Series') {
      return { to: '/library/shows/$seriesId', params: { seriesId: props.item.id } } as const;
    }
    if (props.item.itemType === 'Episode' && props.item.seriesId !== null) {
      return { to: '/library/shows/$seriesId', params: { seriesId: props.item.seriesId } } as const;
    }
    return { to: '/library/items/$itemId', params: { itemId: props.item.id } } as const;
  };

  const renderArtwork = () => (
    <div
      class={cx(styles.artwork, styles.aspect[aspectClass()], styles.homeArtwork)}
      data-aspect={aspectClass()}
    >
      <LibraryImage
        imageId={artworkImageId()}
        alt={`${props.item.name} artwork`}
        class={cx(styles.image, styles.homeImage)}
        fallback={
          showPlayBadge() ? (
            <div class={styles.directResumeFallback} aria-hidden="true" />
          ) : (
            <div class={styles.fallback}>
              <Show
                when={usesTvIcon()}
                fallback={<Film class={styles.fallbackIcon} aria-hidden="true" />}
              >
                <Tv class={styles.fallbackIcon} aria-hidden="true" />
              </Show>
              <span>No artwork</span>
            </div>
          )
        }
      />

      <Show when={progress() !== null}>
        <div
          class={styles.homeProgressTrack}
          role="progressbar"
          aria-label={`${props.item.name} watch progress`}
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(progress() ?? 0)}
        >
          <div class={styles.homeProgressBar} style={{ width: `${progress() ?? 0}%` }} />
        </div>
      </Show>

      <Show when={showPlayBadge()}>
        <span class={styles.playBadge} data-play-badge aria-hidden="true">
          <Play class={styles.playIcon} aria-hidden="true" />
        </span>
      </Show>

      <Show when={props.busy}>
        <div class={styles.homeBusyOverlay} aria-live="polite">
          <LoaderCircle class={styles.homeBusyIcon} aria-hidden="true" />
          <span>Starting…</span>
        </div>
      </Show>
    </div>
  );

  const renderHomeMeta = () => (
    <div class={styles.homeBody}>
      <Link
        {...titleLinkTarget()}
        aria-label={`Open details for ${props.item.name}`}
        class={styles.homeTitleLink}
      >
        <CardTitle id={props.item.id} itemType={props.item.itemType} class={styles.homeTitle}>
          {homeTitle(props.item)}
        </CardTitle>
      </Link>
      <Show when={secondary()}>{(value) => <p class={styles.homeSubtitle}>{value()}</p>}</Show>
    </div>
  );

  if (isDirectResumeCard(props)) {
    return (
      <div class={styles.homeCard}>
        <button
          type="button"
          class={styles.homeCardAction}
          aria-label={props.busy ? `Starting ${props.item.name}` : `Resume ${props.item.name}`}
          aria-busy={props.busy}
          disabled={props.resumeDisabled}
          onClick={props.onResume}
        >
          {renderArtwork()}
        </button>
        {renderHomeMeta()}
      </div>
    );
  }

  return (
    <div class={styles.homeCard}>
      <Link
        to="/library/items/$itemId"
        params={{ itemId: props.item.id }}
        aria-label={`Open ${props.item.name}`}
        class={styles.homeCardAction}
      >
        {renderArtwork()}
      </Link>
      {renderHomeMeta()}
    </div>
  );
}
