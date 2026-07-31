import type { VideoHomeItem, VideoItemDetail } from '@bindings';
import { Link } from '@tanstack/solid-router';
import { Info, Play } from 'lucide-solid';
import { Show, createUniqueId } from 'solid-js';
import type { JSX } from 'solid-js';
import { videoHomePlaybackDecision } from '~utils/videoHomeLayout';

import { Button } from '../ui';
import * as styles from './HomeFeaturedHero.styles';
import { LibraryImage } from './LibraryImage';

export type HomeFeaturedSource = 'continueWatching' | 'nextUp' | 'latestMovies';

export interface HomeFeaturedHeroProps {
  source: HomeFeaturedSource;
  item: VideoHomeItem;
  detail: VideoItemDetail | null;
  busy: boolean;
  playbackDisabled: boolean;
  onPlay: () => void;
}

const sourceEyebrow = (source: HomeFeaturedSource): string => {
  if (source === 'continueWatching') {
    return 'Continue';
  }
  if (source === 'nextUp') {
    return 'Next Up';
  }
  return 'Latest Movie';
};

/**
 * Stable resume-first Home hero. Presentation only: the route owns feature
 * selection, the detail query, and the playback lock. The Home summary item —
 * never the detail response — decides Resume versus Start so the hero and its
 * row card always agree.
 */
export function HomeFeaturedHero(props: HomeFeaturedHeroProps): JSX.Element {
  const headlineId = createUniqueId();

  const isEpisode = () => props.item.itemType === 'Episode';
  const headline = () =>
    isEpisode()
      ? (props.detail?.seriesName ?? props.item.seriesName ?? props.detail?.name ?? props.item.name)
      : (props.detail?.name ?? props.item.name);
  const metadata = () => {
    if (isEpisode()) {
      const episodeName = props.detail?.name ?? props.item.name;
      const seasonNumber = props.item.seasonNumber ?? props.detail?.seasonNumber ?? null;
      const episodeNumber = props.item.episodeNumber ?? props.detail?.episodeNumber ?? null;
      return seasonNumber !== null && episodeNumber !== null
        ? `S${seasonNumber} E${episodeNumber} · ${episodeName}`
        : `Episode · ${episodeName}`;
    }
    const year = props.detail?.productionYear ?? props.item.productionYear;
    return year === null ? 'Movie' : `Movie · ${year}`;
  };
  const imageId = () =>
    props.detail?.backdropImageId ?? props.detail?.artworkImageId ?? props.item.artworkImageId;

  const decision = () => videoHomePlaybackDecision(props.item);
  const actionText = () => (decision().mode === 'resume' ? 'Resume' : 'Play');

  return (
    <section class={styles.hero} aria-labelledby={headlineId}>
      <div class={styles.artwork} aria-hidden="true">
        <LibraryImage
          imageId={imageId()}
          alt=""
          class={styles.image}
          fallback={<div class={styles.imageFallback} />}
        />
        <div class={styles.scrim} />
      </div>
      <div class={styles.content}>
        <p class={styles.eyebrow}>{sourceEyebrow(props.source)}</p>
        <h2 id={headlineId} class={styles.headline}>
          {headline()}
        </h2>
        <p class={styles.metadata}>{metadata()}</p>
        <Show when={props.detail?.overview}>
          {(overview) => <p class={styles.overview}>{overview()}</p>}
        </Show>
        <div class={styles.actions}>
          <Button
            type="button"
            variant="primary"
            aria-label={
              props.busy
                ? `Starting featured ${props.item.name}`
                : `${actionText()} featured ${props.item.name}`
            }
            aria-busy={props.busy}
            disabled={props.busy || props.playbackDisabled}
            onClick={props.onPlay}
            leadingIcon={<Play class={styles.actionIcon} aria-hidden="true" />}
          >
            {props.busy ? 'Starting…' : actionText()}
          </Button>
          <Link
            to="/library/items/$itemId"
            params={{ itemId: props.item.id }}
            class={styles.detailsLink}
          >
            <Info class={styles.actionIcon} aria-hidden="true" />
            Details
          </Link>
        </div>
      </div>
    </section>
  );
}

export function HomeFeaturedHeroSkeleton() {
  return (
    <section class={styles.skeletonHero} role="status" aria-label="Loading featured item">
      <div class={styles.skeletonContent}>
        <div class={styles.skeletonEyebrow} aria-hidden="true" />
        <div class={styles.skeletonHeadline} aria-hidden="true" />
        <div class={styles.skeletonLine} aria-hidden="true" />
        <div class={styles.skeletonActions} aria-hidden="true" />
      </div>
    </section>
  );
}
