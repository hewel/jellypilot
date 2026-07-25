import type { VideoLibraryItem } from '@bindings';
import { cx } from '@styled-system/css';
import { Link } from '@tanstack/solid-router';
import { Check, Film, Heart, Tv } from 'lucide-solid';
import { Show, createEffect, createSignal } from 'solid-js';
import type { JSX } from 'solid-js';
import { imageSource } from '~utils/imageSource';

import * as styles from './LibrarySearchResultRow.styles';

export interface LibrarySearchResultRowProps {
  item: VideoLibraryItem;
}

function zeroPad(value: number): string {
  return String(value).padStart(2, '0');
}

function episodeCode(item: VideoLibraryItem): string | null {
  return item.seasonNumber !== null && item.episodeNumber !== null
    ? `S${zeroPad(item.seasonNumber)}E${zeroPad(item.episodeNumber)}`
    : null;
}

function metadataLine(item: VideoLibraryItem): string {
  if (item.itemType === 'Episode') {
    const parts = [item.seriesName, episodeCode(item) ?? 'Episode'];
    return parts.filter((part): part is string => part !== null).join(' • ');
  }

  const parts = [item.itemType, item.productionYear?.toString() ?? null];
  return parts.filter((part): part is string => part !== null).join(' • ');
}

/**
 * Uniform 16:9 compact row for mixed Movie/Series/Episode search results.
 * The whole row is a single link to the matching detail route.
 */
export function LibrarySearchResultRow(props: LibrarySearchResultRowProps): JSX.Element {
  const [imageFailed, setImageFailed] = createSignal(false);
  const artworkImageId = () => props.item.artworkImageId;

  createEffect(() => {
    artworkImageId();
    setImageFailed(false);
  });

  const linkTarget = () =>
    props.item.itemType === 'Series'
      ? ({ params: { seriesId: props.item.id }, to: '/library/shows/$seriesId' } as const)
      : ({ params: { itemId: props.item.id }, to: '/library/items/$itemId' } as const);

  const usesTvIcon = () => props.item.itemType !== 'Movie';

  const rowAriaLabel = () => `Open ${props.item.name}${props.item.favorite ? ', favorite' : ''}`;

  return (
    <Link {...linkTarget()} aria-label={rowAriaLabel()} class={styles.row}>
      <div class={styles.thumb}>
        <Show
          when={!imageFailed() ? artworkImageId() : null}
          fallback={
            <div class={styles.thumbFallback}>
              <Show
                when={usesTvIcon()}
                fallback={<Film class={styles.fallbackIcon} aria-hidden="true" />}
              >
                <Tv class={styles.fallbackIcon} aria-hidden="true" />
              </Show>
            </div>
          }
        >
          {(imageId) => (
            <img
              src={imageSource(imageId())}
              alt=""
              class={styles.thumbImage}
              loading="lazy"
              onError={() => setImageFailed(true)}
            />
          )}
        </Show>
      </div>
      <div class={styles.copy}>
        <p class={styles.title}>{props.item.name}</p>
        <p class={styles.subtitle}>{metadataLine(props.item)}</p>
      </div>
      <div class={styles.indicators}>
        <Show when={props.item.played}>
          <span class={cx(styles.indicator, styles.playedIndicator)}>
            <Check aria-hidden="true" size={14} />
            Played
          </span>
        </Show>
        <Show when={props.item.favorite}>
          <span class={cx(styles.indicator, styles.favoriteIndicator)}>
            <Heart aria-hidden="true" fill="currentColor" size={14} />
            Favorite
          </span>
        </Show>
      </div>
    </Link>
  );
}
