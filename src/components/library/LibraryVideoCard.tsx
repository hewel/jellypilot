import type { VideoLibraryItem, VideoLibraryKind } from '@bindings';
import { cx } from '@styled-system/css';
import { Link } from '@tanstack/solid-router';
import { Check, Film, Heart, Tv } from 'lucide-solid';
import { Show, createEffect, createMemo } from 'solid-js';
import { imageSource } from '~utils/imageSource';

import { LibraryImage } from './LibraryImage';
import * as styles from './VideoCard.styles';
import { CardTitle, VideoCardSkeleton, type VideoCardAspectClass } from './videoCardShared';

export type LibraryVideoCardProps =
  | {
      item: VideoLibraryItem;
      collectionType?: VideoLibraryKind;
      loading?: false;
    }
  | {
      collectionType?: VideoLibraryKind;
      loading: true;
    };

export function LibraryVideoCard(props: LibraryVideoCardProps) {
  const aspectClass = (): VideoCardAspectClass => {
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

  const linkTarget = () =>
    props.item.itemType === 'Series'
      ? ({ to: '/library/shows/$seriesId', params: { seriesId: props.item.id } } as const)
      : ({ to: '/library/items/$itemId', params: { itemId: props.item.id } } as const);

  const librarySubtitle = () =>
    props.item.productionYear ? props.item.productionYear.toString() : props.item.itemType;

  const usesTvIcon = () =>
    props.collectionType === 'tvshows' ||
    props.item.itemType === 'Series' ||
    props.item.itemType === 'Episode';

  const libraryCardAriaLabel = () =>
    `Open ${props.item.name}${props.item.favorite ? ', favorite' : ''}`;

  const artworkImageId = () => props.item.artworkImageId;
  const artworkUrl = createMemo(() => {
    const imageId = artworkImageId();
    return imageId ? imageSource(imageId) : '';
  });

  createEffect(() => {
    artworkUrl();
  });

  const isPoster = () => aspectClass() === 'poster';

  return (
    <Link {...linkTarget()} aria-label={libraryCardAriaLabel()} class={styles.card}>
      <div class={cx(styles.artwork, styles.aspect[aspectClass()])} data-aspect={aspectClass()}>
        <LibraryImage
          imageId={artworkImageId()}
          alt={`${props.item.name} artwork`}
          class={styles.image}
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
        />

        <Show when={props.item.favorite}>
          <span class={styles.favoriteBadge} aria-hidden="true">
            <Heart class={styles.favoriteIcon} fill="currentColor" aria-hidden="true" />
          </span>
        </Show>

        <Show when={isPoster()}>
          <Show when={props.item.played}>
            <span class={styles.overlayPlayedBadge} role="img" aria-label="Played">
              <Check class={styles.playedIcon} aria-hidden="true" />
            </span>
          </Show>
          <div class={styles.overlay}>
            <CardTitle id={props.item.id} itemType={props.item.itemType} class={styles.title}>
              {props.item.name}
            </CardTitle>
            <p class={styles.subtitle}>{librarySubtitle()}</p>
          </div>
        </Show>
      </div>
      <Show when={!isPoster()}>
        <div class={styles.body}>
          <div class={styles.copy}>
            <CardTitle id={props.item.id} itemType={props.item.itemType} class={styles.title}>
              {props.item.name}
            </CardTitle>
            <p class={styles.subtitle}>{librarySubtitle()}</p>
          </div>
          <Show when={props.item.played}>
            <span class={styles.playedBadge} role="img" aria-label="Played">
              <Check class={styles.playedIcon} aria-hidden="true" />
            </span>
          </Show>
        </div>
      </Show>
    </Link>
  );
}
