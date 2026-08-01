import { Carousel } from '@ark-ui/solid/carousel';
import type { VideoLibraryItem } from '@bindings';
import { Link } from '@tanstack/solid-router';
import { ChevronLeft, ChevronRight } from 'lucide-solid';
import { For, type JSX, Show, createSignal } from 'solid-js';

import { Button } from '../ui';
import * as styles from './DetailRecommendationShelf.styles';
import { LibraryImage } from './LibraryImage';
import { LibraryStatusPanel } from './shared';

function RecommendationCard(props: { item: VideoLibraryItem }): JSX.Element {
  const body = (
    <>
      <div class={styles.poster}>
        <LibraryImage
          imageId={props.item.artworkImageId}
          alt={`${props.item.name} poster`}
          class={styles.posterFallback}
          loading="lazy"
          fallback={
            <div class={styles.posterFallback} aria-hidden="true">
              {props.item.name.charAt(0)}
            </div>
          }
        />
      </div>
      <p class={styles.cardTitle}>{props.item.name}</p>
      <div class={styles.cardMeta}>
        <span>{cardMeta(props.item)}</span>
        <Show when={props.item.played}>
          <span class={styles.cardMetaSeparator} aria-hidden="true">
            ·
          </span>
          <span class={styles.cardPlayed}>Played</span>
        </Show>
        <Show when={props.item.favorite}>
          <span class={styles.cardMetaSeparator} aria-hidden="true">
            ·
          </span>
          <span class={styles.cardFavorite}>Favorite</span>
        </Show>
      </div>
    </>
  );

  return (
    <Show
      when={props.item.itemType === 'Series'}
      fallback={
        <Link
          to="/library/items/$itemId"
          params={{ itemId: props.item.id }}
          class={styles.card}
          aria-label={props.item.name}
        >
          {body}
        </Link>
      }
    >
      <Link
        to="/library/shows/$seriesId"
        params={{ seriesId: props.item.id }}
        class={styles.card}
        aria-label={props.item.name}
      >
        {body}
      </Link>
    </Show>
  );
}

function cardMeta(item: VideoLibraryItem): string {
  if (item.productionYear !== null) {
    return item.productionYear.toString();
  }
  return item.itemType;
}

/**
 * Server-backed "More like this" shelf. Uses an Ark carousel for equal-width
 * portrait cards; each card routes Series to the show page and Movie/Episode to
 * the item page. Provider order is authoritative — no indicators, autoplay, or
 * "See all" control.
 */
export function DetailRecommendationShelf(props: {
  title: string;
  items: readonly VideoLibraryItem[];
}): JSX.Element {
  const slidesPerPage = 5;
  const [page, setPage] = createSignal(0);
  const slideCount = () => props.items.length;
  const pageCount = () => Math.max(1, Math.ceil(slideCount() / slidesPerPage));
  const canPrev = () => page() > 0;
  const canNext = () => page() < pageCount() - 1;

  return (
    <section class={styles.section} aria-label={props.title}>
      <div class={styles.header}>
        <h2 class={styles.heading}>{props.title}</h2>
        <div class={styles.controls}>
          <button
            type="button"
            class={styles.arrow}
            aria-label="Previous recommendations"
            disabled={!canPrev()}
            onClick={() => setPage((value) => Math.max(0, value - 1))}
          >
            <ChevronLeft class={styles.arrowIcon} aria-hidden="true" />
          </button>
          <button
            type="button"
            class={styles.arrow}
            aria-label="Next recommendations"
            disabled={!canNext()}
            onClick={() => setPage((value) => Math.min(pageCount() - 1, value + 1))}
          >
            <ChevronRight class={styles.arrowIcon} aria-hidden="true" />
          </button>
        </div>
      </div>

      <Carousel.Root
        autoSize
        slideCount={slideCount()}
        slidesPerPage={slidesPerPage}
        spacing="1rem"
        page={page()}
        onPageChange={(details) => setPage(details.page)}
      >
        <Carousel.ItemGroup class={styles.itemGroup}>
          <For each={props.items}>
            {(item, index) => (
              <Carousel.Item class={styles.item} index={index()}>
                <RecommendationCard item={item} />
              </Carousel.Item>
            )}
          </For>
        </Carousel.ItemGroup>
      </Carousel.Root>
    </section>
  );
}

export function DetailRecommendationError(props: {
  title: string;
  message: string;
  onRetry: () => void;
}): JSX.Element {
  return (
    <section class={styles.section} aria-label={props.title}>
      <div class={styles.statusWrap}>
        <LibraryStatusPanel title={props.title} description={props.message} />
        <Button
          type="button"
          variant="secondary"
          class={styles.retryButton}
          onClick={props.onRetry}
        >
          Retry
        </Button>
      </div>
    </section>
  );
}

export function DetailRecommendationShelfSkeleton(props: { title: string }): JSX.Element {
  return (
    <section class={styles.skeletonSection} role="status" aria-label={`Loading ${props.title}`}>
      <div class={styles.skeletonHeading} aria-hidden="true" />
      <div class={styles.skeletonRow} aria-hidden="true">
        <For each={[0, 1, 2, 3, 4]}>{() => <div class={styles.skeletonCard} />}</For>
      </div>
    </section>
  );
}
