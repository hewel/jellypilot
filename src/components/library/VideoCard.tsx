import type { VideoLibraryItem } from '@bindings';
import { cx } from '@styled-system/css';
import { Link } from '@tanstack/solid-router';
import { Check, Film, Heart, LoaderCircle, Play, Tv } from 'lucide-solid';
import { Show } from 'solid-js';

import { LibraryImage } from './LibraryImage';
import * as styles from './VideoCard.styles';
import {
  videoCardActionLabel,
  videoCardAriaLabel,
  videoCardDetailsTarget,
  videoCardIcon,
  videoCardTitle,
  videoCardTitleTarget,
} from './videoCardModel';
import { CardTitle, type VideoCardAspectClass } from './videoCardShared';

export type VideoCardAction =
  | { kind: 'open' }
  | { kind: 'play'; busy?: boolean; disabled?: boolean; onPlay: () => void };

export interface VideoCardProps {
  item: VideoLibraryItem;
  aspect: VideoCardAspectClass;
  action: VideoCardAction;
  /** overlay = copy on artwork (poster grids); below = copy under artwork. */
  copy: 'overlay' | 'below';
  subtitle?: string | null;
  progress?: number | null;
  badges?: { favorite?: boolean; played?: boolean };
}

/**
 * The single video card: direct-playback home cards (action 'play'), split-link
 * cards with copy under the artwork (action 'open', copy 'below'), and
 * poster-grid cards with copy overlaid on the artwork (action 'open', copy
 * 'overlay'). All derivations come from videoCardModel.
 */
export function VideoCard(props: VideoCardProps) {
  const busy = () => props.action.kind === 'play' && props.action.busy === true;
  const showPlayBadge = () => props.action.kind === 'play' && !busy();
  const showFavoriteBadge = () => props.badges?.favorite === true && props.item.favorite;
  const showPlayedBadge = () => props.badges?.played === true && props.item.played;

  const iconFallback = () => (
    <div class={styles.fallback}>
      <Show
        when={videoCardIcon(props.item) === 'tv'}
        fallback={<Film class={styles.fallbackIcon} aria-hidden="true" />}
      >
        <Tv class={styles.fallbackIcon} aria-hidden="true" />
      </Show>
      <span>No artwork</span>
    </div>
  );

  const renderFramedArtwork = () => (
    <div
      class={cx(styles.artwork, styles.aspect[props.aspect], styles.homeArtwork)}
      data-aspect={props.aspect}
    >
      <LibraryImage
        imageId={props.item.artworkImageId}
        alt={`${props.item.name} artwork`}
        class={cx(styles.image, styles.homeImage)}
        fallback={
          showPlayBadge() ? (
            <div class={styles.directPlaybackFallback} aria-hidden="true" />
          ) : (
            iconFallback()
          )
        }
      />

      <Show when={props.progress != null}>
        <div
          class={styles.homeProgressTrack}
          role="progressbar"
          aria-label={`${props.item.name} watch progress`}
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(props.progress ?? 0)}
        >
          <div class={styles.homeProgressBar} style={{ width: `${props.progress ?? 0}%` }} />
        </div>
      </Show>

      <Show when={showFavoriteBadge()}>
        <span class={styles.favoriteBadge} aria-hidden="true">
          <Heart class={styles.favoriteIcon} fill="currentColor" aria-hidden="true" />
        </span>
      </Show>

      <Show when={showPlayBadge()}>
        <span class={styles.playBadge} data-play-badge aria-hidden="true">
          <Play class={styles.playIcon} aria-hidden="true" />
        </span>
      </Show>

      <Show when={busy()}>
        <div class={styles.homeBusyOverlay} aria-live="polite">
          <LoaderCircle class={styles.homeBusyIcon} aria-hidden="true" />
          <span>Starting…</span>
        </div>
      </Show>
    </div>
  );

  const renderBelowMeta = () => (
    <div class={styles.belowMeta}>
      <div class={styles.belowCopy}>
        <Link
          {...videoCardTitleTarget(props.item)}
          aria-label={`Open details for ${props.item.name}`}
          class={styles.homeTitleLink}
        >
          <CardTitle id={props.item.id} itemType={props.item.itemType} class={styles.title}>
            {videoCardTitle(props.item)}
          </CardTitle>
        </Link>
        <Show when={props.subtitle}>{(value) => <p class={styles.homeSubtitle}>{value()}</p>}</Show>
      </div>
      <Show when={showPlayedBadge()}>
        <span class={styles.playedBadge} role="img" aria-label="Played">
          <Check class={styles.playedIcon} aria-hidden="true" />
        </span>
      </Show>
    </div>
  );

  if (props.action.kind === 'play') {
    return (
      <div class={styles.homeCard}>
        <button
          type="button"
          class={styles.homeCardAction}
          aria-label={videoCardActionLabel(props.item, props.action.busy === true)}
          aria-busy={props.action.busy}
          disabled={props.action.busy || props.action.disabled}
          onClick={props.action.onPlay}
        >
          {renderFramedArtwork()}
        </button>
        {renderBelowMeta()}
      </div>
    );
  }

  if (props.copy === 'below') {
    return (
      <div class={styles.homeCard}>
        <Link
          {...videoCardDetailsTarget(props.item)}
          aria-label={videoCardAriaLabel(props.item)}
          class={styles.homeCardAction}
        >
          {renderFramedArtwork()}
        </Link>
        {renderBelowMeta()}
      </div>
    );
  }

  return (
    <Link
      {...videoCardDetailsTarget(props.item)}
      aria-label={videoCardAriaLabel(props.item)}
      class={styles.card}
    >
      <div class={cx(styles.artwork, styles.aspect[props.aspect])} data-aspect={props.aspect}>
        <LibraryImage
          imageId={props.item.artworkImageId}
          alt={`${props.item.name} artwork`}
          class={styles.image}
          fallback={iconFallback()}
        />

        <Show when={showFavoriteBadge()}>
          <span class={styles.favoriteBadge} aria-hidden="true">
            <Heart class={styles.favoriteIcon} fill="currentColor" aria-hidden="true" />
          </span>
        </Show>

        <Show when={showPlayedBadge()}>
          <span class={styles.overlayPlayedBadge} role="img" aria-label="Played">
            <Check class={styles.playedIcon} aria-hidden="true" />
          </span>
        </Show>

        <div class={styles.overlay}>
          <CardTitle id={props.item.id} itemType={props.item.itemType} class={styles.title}>
            {videoCardTitle(props.item)}
          </CardTitle>
          <Show when={props.subtitle}>{(value) => <p class={styles.subtitle}>{value()}</p>}</Show>
        </div>
      </div>
    </Link>
  );
}
