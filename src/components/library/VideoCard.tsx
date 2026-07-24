import type { VideoHomeItem, VideoLibraryItem, VideoLibraryKind } from '@bindings';
import { cx } from '@styled-system/css';
import { Link } from '@tanstack/solid-router';
import { Match } from 'effect';
import { Check, Film, Heart, LoaderCircle, Tv } from 'lucide-solid';
import { Show, createEffect, createSignal } from 'solid-js';
import { imageSource } from '~utils/imageSource';
import {
  isValidVideoHomeResumePosition,
  videoHomeAspect,
  type VideoHomeAspect,
  type VideoHomeRowKind,
} from '~utils/videoHomeLayout';

import * as styles from './VideoCard.styles';

export type VideoCardAspectClass = VideoHomeAspect;

interface HomeVideoCardProps {
  kind: 'home';
  item: VideoHomeItem;
  rowKind: VideoHomeRowKind;
  busy?: boolean;
  resumeDisabled?: boolean;
  onResume?: () => void;
  loading?: false;
}

export type VideoCardProps =
  | HomeVideoCardProps
  | {
      kind: 'library';
      item: VideoLibraryItem;
      collectionType?: VideoLibraryKind;
      loading?: false;
    }
  | {
      kind: 'library';
      collectionType?: VideoLibraryKind;
      loading: true;
    };

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

export function VideoCard(props: VideoCardProps) {
  const aspectClass = (): VideoCardAspectClass => {
    if (props.kind === 'home') {
      return videoHomeAspect(props.rowKind);
    }
    if (props.loading) {
      return 'poster';
    }
    return props.collectionType === 'tvshows' ||
      props.item.itemType === 'Series' ||
      props.item.itemType === 'Movie'
      ? 'poster'
      : 'video';
  };

  if (props.kind === 'library' && props.loading) {
    return <VideoCardSkeleton aspectClass={aspectClass()} />;
  }

  const linkTarget = () =>
    props.kind === 'library' && props.item.itemType === 'Series'
      ? ({ to: '/library/shows/$seriesId', params: { seriesId: props.item.id } } as const)
      : ({ to: '/library/items/$itemId', params: { itemId: props.item.id } } as const);

  const librarySubtitle = () => {
    if (props.kind === 'home') {
      return null;
    }
    return props.item.productionYear ? props.item.productionYear.toString() : props.item.itemType;
  };

  const usesTvIcon = () =>
    (props.kind === 'library' && props.collectionType === 'tvshows') ||
    props.item.itemType === 'Series' ||
    props.item.itemType === 'Episode';

  const libraryCardAriaLabel = () =>
    `Open ${props.item.name}${props.item.favorite ? ', favorite' : ''}`;
  const [imageFailed, setImageFailed] = createSignal(false);
  const artworkImageId = () => props.item.artworkImageId;

  createEffect(() => {
    artworkImageId();
    setImageFailed(false);
  });

  const isPoster = () => aspectClass() === 'poster';
  const progress = () =>
    props.kind === 'home' && props.rowKind === 'continueWatching'
      ? videoHomeProgress(props.item)
      : null;
  const secondary = () =>
    props.kind === 'home'
      ? homeSecondary({ item: props.item, rowKind: props.rowKind })
      : librarySubtitle();

  const renderContents = () => (
    <>
      <div
        class={cx(
          styles.artwork,
          styles.aspect[aspectClass()],
          props.kind === 'home' && styles.homeArtwork,
        )}
        data-aspect={aspectClass()}
      >
        <Show
          when={!imageFailed() ? artworkImageId() : null}
          fallback={
            <div class={styles.fallback}>
              <Show
                when={usesTvIcon()}
                fallback={<Film class={styles.fallbackIcon} aria-hidden="true" />}
              >
                <Tv class={styles.fallbackIcon} aria-hidden="true" />
              </Show>
              <span>No artwork</span>
            </div>
          }
        >
          {(imageId) => (
            <img
              src={imageSource(imageId())}
              alt={`${props.item.name} artwork`}
              class={cx(styles.image, props.kind === 'home' && styles.homeImage)}
              onError={() => setImageFailed(true)}
            />
          )}
        </Show>

        <Show when={props.kind === 'library' && props.item.favorite}>
          <span class={styles.favoriteBadge} aria-hidden="true">
            <Heart class={styles.favoriteIcon} fill="currentColor" aria-hidden="true" />
          </span>
        </Show>

        <Show when={props.kind === 'library' && isPoster()}>
          <Show when={props.item.played}>
            <span class={styles.overlayPlayedBadge} role="img" aria-label="Played">
              <Check class={styles.playedIcon} aria-hidden="true" />
            </span>
          </Show>
          <div class={styles.overlay}>
            <p class={styles.title}>{props.item.name}</p>
            <p class={styles.subtitle}>{librarySubtitle()}</p>
          </div>
        </Show>

        <Show when={props.kind === 'home' && progress() !== null}>
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

        <Show when={props.kind === 'home' && props.busy}>
          <div class={styles.homeBusyOverlay} aria-live="polite">
            <LoaderCircle class={styles.homeBusyIcon} aria-hidden="true" />
            <span>Starting…</span>
          </div>
        </Show>
      </div>

      <Show
        when={props.kind === 'home'}
        fallback={
          <Show when={!isPoster()}>
            <div class={styles.body}>
              <div class={styles.copy}>
                <p class={styles.title}>{props.item.name}</p>
                <p class={styles.subtitle}>{librarySubtitle()}</p>
              </div>
              <Show when={props.item.played}>
                <span class={styles.playedBadge} role="img" aria-label="Played">
                  <Check class={styles.playedIcon} aria-hidden="true" />
                </span>
              </Show>
            </div>
          </Show>
        }
      >
        <div class={styles.homeBody}>
          <p class={styles.homeTitle}>{homeTitle(props.item)}</p>
          <Show when={secondary()}>{(value) => <p class={styles.homeSubtitle}>{value()}</p>}</Show>
        </div>
      </Show>
    </>
  );

  if (
    props.kind === 'home' &&
    props.rowKind === 'continueWatching' &&
    isValidVideoHomeResumePosition(props.item.resumePositionSeconds, props.item.runtimeSeconds) &&
    props.onResume
  ) {
    return (
      <button
        type="button"
        class={styles.homeCard}
        aria-label={props.busy ? `Starting ${props.item.name}` : `Resume ${props.item.name}`}
        aria-busy={props.busy}
        disabled={props.resumeDisabled}
        onClick={props.onResume}
      >
        {renderContents()}
      </button>
    );
  }

  return (
    <Link
      {...linkTarget()}
      aria-label={props.kind === 'home' ? `Open ${props.item.name}` : libraryCardAriaLabel()}
      class={props.kind === 'home' ? styles.homeCard : styles.card}
    >
      {renderContents()}
    </Link>
  );
}

function VideoCardSkeleton(props: { aspectClass: VideoCardAspectClass }) {
  return (
    <div class={styles.card} aria-hidden="true">
      <div
        class={cx(styles.artwork, styles.aspect[props.aspectClass], styles.skeleton)}
        data-aspect={props.aspectClass}
      />
      <Show when={props.aspectClass === 'video'}>
        <div class={styles.skeletonBody}>
          <div class={styles.skeletonTitle} />
          <div class={styles.skeletonSubtitle} />
        </div>
      </Show>
    </div>
  );
}
