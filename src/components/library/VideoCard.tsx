import { Link } from '@jellypilot/ui';
import { Check, Film, Heart, Tv } from 'lucide-solid';
import { Show, createEffect, createSignal } from 'solid-js';

import type { VideoHomeItem, VideoLibraryItem, VideoLibraryKind } from '../../bindings';
import { imageSource } from '../../utils/imageSource';

import * as styles from './VideoCard.css';

export type VideoCardAspectClass = 'poster' | 'video';

export type VideoCardProps =
  | {
      kind: 'home';
      item: VideoHomeItem;
      aspectClass: VideoCardAspectClass;
      loading?: false;
    }
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

export function VideoCard(props: VideoCardProps) {
  const aspectClass = (): VideoCardAspectClass => {
    if (props.kind === 'home') {
      return props.aspectClass;
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

  if (props.loading) {
    return <VideoCardSkeleton aspectClass={aspectClass()} />;
  }

  const href = () => {
    if (props.kind === 'home') {
      return `/library/items/${props.item.id}`;
    }
    return props.item.itemType === 'Series'
      ? `/library/shows/${props.item.id}`
      : `/library/items/${props.item.id}`;
  };

  const subtitle = () => {
    if (props.kind === 'home') {
      const item = props.item;
      if (!item.seriesName) {
        return item.productionYear ? `${item.itemType} · ${item.productionYear}` : item.itemType;
      }
      const number =
        item.seasonNumber !== null && item.episodeNumber !== null
          ? `S${item.seasonNumber.toString().padStart(2, '0')}E${item.episodeNumber.toString().padStart(2, '0')}`
          : item.itemType;
      return `${item.seriesName} · ${number}`;
    }

    const year = props.item.productionYear
      ? props.item.productionYear.toString()
      : props.item.itemType;
    return year;
  };

  const usesTvIcon = () =>
    (props.kind === 'library' && props.collectionType === 'tvshows') ||
    props.item.itemType === 'Series' ||
    props.item.itemType === 'Episode';

  const cardAriaLabel = () => `Open ${props.item.name}${props.item.favorite ? ', favorite' : ''}`;
  const [imageFailed, setImageFailed] = createSignal(false);
  const artworkImageId = () => props.item.artworkImageId;

  createEffect(() => {
    artworkImageId();
    setImageFailed(false);
  });

  return (
    <Link href={href()} aria-label={cardAriaLabel()} class={styles.card}>
      <div class={`${styles.artwork} ${styles.aspect[aspectClass()]}`}>
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
              class={styles.image}
              loading="lazy"
              onError={() => setImageFailed(true)}
            />
          )}
        </Show>
        <Show when={props.item.favorite}>
          <span class={styles.favoriteBadge} aria-hidden="true">
            <Heart class={styles.favoriteIcon} fill="currentColor" aria-hidden="true" />
          </span>
        </Show>
      </div>
      <div class={styles.body}>
        <div class={styles.copy}>
          <p class={styles.title}>{props.item.name}</p>
          <p class={styles.subtitle}>{subtitle()}</p>
        </div>
        <Show when={props.item.played}>
          <span class={styles.playedBadge} role="img" aria-label="Played">
            <Check class={styles.playedIcon} aria-hidden="true" />
          </span>
        </Show>
      </div>
    </Link>
  );
}

function VideoCardSkeleton(props: { aspectClass: VideoCardAspectClass }) {
  return (
    <div class={styles.card} aria-hidden="true">
      <div class={`${styles.artwork} ${styles.aspect[props.aspectClass]} ${styles.skeleton}`} />
      <div class={styles.skeletonBody}>
        <div class={styles.skeletonTitle} />
        <div class={styles.skeletonSubtitle} />
      </div>
    </div>
  );
}
