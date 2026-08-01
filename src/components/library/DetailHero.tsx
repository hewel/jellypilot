import { Link } from '@tanstack/solid-router';
import { ChevronLeft, Film, Star, Tv } from 'lucide-solid';
import { For, type JSX, Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';

import * as styles from './DetailHero.styles';
import { LibraryImage } from './LibraryImage';

export interface DetailHeroInfoRow {
  label: string;
  value: string;
}

export interface DetailHeroModel {
  itemId: string;
  name: string;
  itemType: 'Movie' | 'Episode' | 'Series';
  artworkImageId: string | null;
  backdropImageId: string | null;
  productionYear: number | null;
  runtime: string | null;
  overview: string | null;
  communityRating: number | null;
  officialRating: string | null;
  genres: readonly string[];
  creators: readonly string[];
  cast: readonly string[];
  seriesName: string | null;
  seriesId: string | null;
  episodeCode: string | null;
}

const MAX_CREATORS = 2;
const MAX_CAST = 4;

function typeLabel(itemType: DetailHeroModel['itemType']) {
  return itemType === 'Series' ? 'Series' : itemType === 'Episode' ? 'Episode' : 'Movie';
}

function communityRatingLabel(rating: number) {
  return `${rating.toFixed(1)}/10`;
}

/**
 * Cinematic detail hero shared by the item and show detail routes: a full-bleed
 * backdrop carries identity while a portrait poster and the copy column sit
 * directly on a deep theme scrim, and a summary surface below lists Genres /
 * Creators / Cast plus a deferred technical row.
 */
export function DetailHero(props: {
  titleId: string;
  model: DetailHeroModel;
  technicalRows?: readonly DetailHeroInfoRow[];
  technicalRowsLoading?: boolean;
  actions: () => JSX.Element;
  onBack: () => void;
}) {
  let overviewRef: HTMLParagraphElement | undefined;
  const [expanded, setExpanded] = createSignal(false);
  const [overflowing, setOverflowing] = createSignal(false);

  // Reset expansion whenever the viewed item changes.
  createEffect(() => {
    props.model.itemId;
    setExpanded(false);
  });

  const measure = () => {
    const el = overviewRef;
    if (!el || expanded()) {
      return;
    }
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
  };

  onMount(() => {
    const el = overviewRef;
    if (!el) {
      return;
    }
    const observer = new ResizeObserver(() => measure());
    observer.observe(el);
    measure();
    onCleanup(() => observer.disconnect());
  });

  const hasSeriesMeta = () => Boolean(props.model.seriesName) || Boolean(props.model.episodeCode);
  const showRating = () => {
    const rating = props.model.communityRating;
    return rating !== null && Number.isFinite(rating) && rating >= 0 && rating <= 10;
  };
  const creators = () => props.model.creators.slice(0, MAX_CREATORS);
  const extraCreators = () => Math.max(0, props.model.creators.length - MAX_CREATORS);
  const cast = () => props.model.cast.slice(0, MAX_CAST);
  const extraCast = () => Math.max(0, props.model.cast.length - MAX_CAST);
  const technicalRows = () => props.technicalRows ?? [];
  const hasSummary = () =>
    props.model.genres.length > 0 ||
    props.model.creators.length > 0 ||
    props.model.cast.length > 0 ||
    technicalRows().length > 0 ||
    props.technicalRowsLoading === true;

  return (
    <>
      <section class={styles.hero} aria-labelledby={props.titleId}>
        <div class={styles.backdrop}>
          <LibraryImage
            imageId={props.model.backdropImageId}
            alt={`${props.model.name} backdrop`}
            class={styles.backdropImage}
            fallback={
              <div class={styles.backdropFallback} aria-hidden="true">
                {props.model.name.charAt(0)}
              </div>
            }
          />
          <div class={styles.scrim} aria-hidden="true" />
        </div>

        <button type="button" class={styles.backLink} aria-label="Back" onClick={props.onBack}>
          <ChevronLeft class={styles.icon4} aria-hidden="true" />
          Back
        </button>

        <div class={styles.content}>
          <div class={styles.poster} data-detail-poster="">
            <LibraryImage
              imageId={props.model.artworkImageId}
              alt={`${props.model.name} poster`}
              class={styles.backdropImage}
              fallback={
                <div class={styles.posterFallback} aria-hidden="true">
                  {props.model.name.charAt(0)}
                </div>
              }
            />
          </div>

          <div class={styles.copy}>
            <div class={styles.metaRow}>
              <span class={styles.chip}>
                <Show
                  when={props.model.itemType === 'Movie'}
                  fallback={<Tv class={styles.icon4} aria-hidden="true" />}
                >
                  <Film class={styles.icon4} aria-hidden="true" />
                </Show>
                {typeLabel(props.model.itemType)}
              </span>
              <Show when={props.model.productionYear}>
                {(year) => <span class={styles.metaText}>{year()}</span>}
              </Show>
              <Show when={props.model.runtime}>
                {(runtime) => <span class={styles.metaText}>{runtime()}</span>}
              </Show>
              <Show when={showRating()}>
                <span class={`${styles.chip} ${styles.ratingChip}`}>
                  <Star class={styles.icon4} aria-hidden="true" />
                  {communityRatingLabel(props.model.communityRating as number)}
                </span>
              </Show>
              <Show when={props.model.officialRating}>
                {(rating) => <span class={styles.chip}>{rating()}</span>}
              </Show>
            </div>

            <h1 id={props.titleId} class={styles.title}>
              {props.model.name}
            </h1>

            <Show when={hasSeriesMeta()}>
              <p class={styles.metaLine}>
                <Show when={props.model.seriesName}>
                  {(seriesName) => (
                    <Show when={props.model.seriesId} fallback={<span>{seriesName()}</span>}>
                      {(seriesId) => (
                        <Link
                          to="/library/shows/$seriesId"
                          params={{ seriesId: seriesId() }}
                          class={styles.metaLink}
                        >
                          {seriesName()}
                        </Link>
                      )}
                    </Show>
                  )}
                </Show>
                <Show when={props.model.seriesName && props.model.episodeCode}>
                  <span aria-hidden="true">·</span>
                </Show>
                <Show when={props.model.episodeCode}>
                  {(episodeCode) => <span>{episodeCode()}</span>}
                </Show>
              </p>
            </Show>

            <Show when={props.model.overview}>
              {(overview) => (
                <div class={styles.overviewWrap}>
                  <p
                    ref={overviewRef}
                    class={`${styles.overview} ${expanded() ? '' : styles.overviewClamped}`}
                  >
                    {overview()}
                  </p>
                  <Show when={overflowing()}>
                    <button
                      type="button"
                      class={styles.overviewToggle}
                      aria-expanded={expanded()}
                      onClick={() => setExpanded((value) => !value)}
                    >
                      {expanded() ? 'Less' : 'More'}
                    </button>
                  </Show>
                </div>
              )}
            </Show>

            <div class={styles.actions}>{props.actions()}</div>
          </div>
        </div>
      </section>

      <Show when={hasSummary()}>
        <div class={styles.summary}>
          <Show
            when={
              props.model.genres.length > 0 ||
              props.model.creators.length > 0 ||
              props.model.cast.length > 0
            }
          >
            <div class={styles.summaryColumns}>
              <Show when={props.model.genres.length > 0}>
                <div class={styles.summaryColumn}>
                  <p class={styles.summaryLabel}>Genres</p>
                  <p class={styles.summaryValues} aria-label={props.model.genres.join(', ')}>
                    <For each={props.model.genres}>
                      {(genre, index) => (
                        <>
                          <Show when={index() > 0}>
                            <span class={styles.summarySeparator} aria-hidden="true">
                              •
                            </span>
                          </Show>
                          <span class={styles.summaryValue}>{genre}</span>
                        </>
                      )}
                    </For>
                  </p>
                </div>
              </Show>
              <Show when={props.model.creators.length > 0}>
                <div class={styles.summaryColumn}>
                  <p class={styles.summaryLabel}>Creators</p>
                  <p class={styles.summaryValues} aria-label={props.model.creators.join(', ')}>
                    <For each={creators()}>
                      {(creator, index) => (
                        <>
                          <Show when={index() > 0}>
                            <span class={styles.summarySeparator} aria-hidden="true">
                              •
                            </span>
                          </Show>
                          <span class={styles.summaryValue}>{creator}</span>
                        </>
                      )}
                    </For>
                    <Show when={extraCreators() > 0}>
                      <span class={styles.summaryMore}>+{extraCreators()} more</span>
                    </Show>
                  </p>
                </div>
              </Show>
              <Show when={props.model.cast.length > 0}>
                <div class={styles.summaryColumn}>
                  <p class={styles.summaryLabel}>Cast</p>
                  <p class={styles.summaryValues} aria-label={props.model.cast.join(', ')}>
                    <For each={cast()}>
                      {(actor, index) => (
                        <>
                          <Show when={index() > 0}>
                            <span class={styles.summarySeparator} aria-hidden="true">
                              •
                            </span>
                          </Show>
                          <span class={styles.summaryValue}>{actor}</span>
                        </>
                      )}
                    </For>
                    <Show when={extraCast() > 0}>
                      <span class={styles.summaryMore}>+{extraCast()} more</span>
                    </Show>
                  </p>
                </div>
              </Show>
            </div>
          </Show>

          <Show when={technicalRows().length > 0 || props.technicalRowsLoading}>
            <dl class={styles.technicalRows}>
              <Show
                when={technicalRows().length > 0}
                fallback={<span class={styles.technicalLoading}>Loading audio and subtitles…</span>}
              >
                <For each={technicalRows()}>
                  {(row) => (
                    <div class={styles.technicalRow}>
                      <dt class={styles.technicalLabel}>{row.label}</dt>
                      <dd class={styles.technicalValue}>{row.value}</dd>
                    </div>
                  )}
                </For>
              </Show>
            </dl>
          </Show>
        </div>
      </Show>
    </>
  );
}

export function DetailHeroSkeleton() {
  return (
    <>
      <section class={styles.skeletonHero} role="status" aria-label="Loading item detail">
        <div class={styles.skeletonBackdrop} aria-hidden="true" />
        <div class={styles.skeletonContent}>
          <div class={styles.skeletonPoster} aria-hidden="true" />
          <div class={styles.skeletonPanel}>
            <div class={styles.skeletonBadge} aria-hidden="true" />
            <div class={styles.skeletonTitle} aria-hidden="true" />
            <div class={styles.skeletonLine} aria-hidden="true" />
            <div class={styles.skeletonActions} aria-hidden="true" />
          </div>
        </div>
      </section>
      <div class={styles.skeletonSummary} aria-hidden="true">
        <div class={styles.skeletonSummaryColumn}>
          <div class={styles.skeletonSummaryLabel} />
          <div class={styles.skeletonSummaryLine} />
        </div>
        <div class={styles.skeletonSummaryColumn}>
          <div class={styles.skeletonSummaryLabel} />
          <div class={styles.skeletonSummaryLine} />
        </div>
        <div class={styles.skeletonSummaryColumn}>
          <div class={styles.skeletonSummaryLabel} />
          <div class={styles.skeletonSummaryLine} />
        </div>
      </div>
    </>
  );
}
