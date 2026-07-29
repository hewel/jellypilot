import { Link } from '@tanstack/solid-router';
import { ChevronLeft } from 'lucide-solid';
import { For, type JSX, Show, createMemo } from 'solid-js';
import { imageSource } from '~utils/imageSource';

import { StatusBadge } from '../ui';
import * as styles from './DetailHero.styles';

export interface DetailHeroInfoRow {
  label: string;
  value: string;
}

/**
 * Cinematic detail hero shared by the item and show detail routes: full-bleed
 * backdrop with layered scrims, glass badges, display title, genre line,
 * playback actions, and an overview + glass info panel grid.
 */
export function DetailHero(props: {
  titleId: string;
  name: string;
  typeLabel: string;
  typeIcon: JSX.Element;
  imageId: string | null;
  year: number | null;
  runtime: string | null;
  watchedPercent: number | null;
  played: boolean;
  favorite: boolean;
  genres: string[];
  overview: string | null;
  infoRows: DetailHeroInfoRow[];
  seriesName?: string | null;
  seriesId?: string | null;
  episodeCode?: string | null;
  actions: JSX.Element;
  onBack: () => void;
}) {
  const hasSeriesMeta = () => Boolean(props.seriesName) || Boolean(props.episodeCode);
  const imageUrl = createMemo(() => (props.imageId ? imageSource(props.imageId) : ''));

  return (
    <section class={styles.hero} aria-labelledby={props.titleId}>
      <div class={styles.backdrop}>
        <Show
          when={imageUrl()}
          fallback={
            <div class={styles.backdropFallback} aria-hidden="true">
              {props.name.charAt(0)}
            </div>
          }
        >
          {(url) => <img src={url()} alt={`${props.name} backdrop`} class={styles.backdropImage} />}
        </Show>
        <div class={styles.scrim} aria-hidden="true" />
      </div>

      <button type="button" class={styles.backLink} aria-label="Back" onClick={props.onBack}>
        <ChevronLeft class={styles.icon4} aria-hidden="true" />
        Back to library
      </button>

      <div class={styles.content}>
        <div class={styles.badgeRow}>
          <span class={styles.chip}>
            {props.typeIcon}
            {props.typeLabel}
          </span>
          <Show when={props.year}>{(year) => <span class={styles.metaText}>{year()}</span>}</Show>
          <Show when={props.runtime}>
            {(runtime) => <span class={styles.metaText}>{runtime()}</span>}
          </Show>
          <Show when={props.watchedPercent}>
            {(percent) => <span class={styles.chip}>{percent()}% watched</span>}
          </Show>
          <StatusBadge variant={props.played ? 'success' : 'neutral'}>
            {props.played ? 'Played' : 'Unplayed'}
          </StatusBadge>
          <StatusBadge variant={props.favorite ? 'success' : 'neutral'}>
            {props.favorite ? 'Favorite' : 'Not favorite'}
          </StatusBadge>
        </div>

        <h1 id={props.titleId} class={styles.title}>
          {props.name}
        </h1>

        <Show when={hasSeriesMeta()}>
          <p class={styles.metaLine}>
            <Show when={props.seriesName}>
              {(seriesName) => (
                <>
                  <Show when={props.seriesId} fallback={<span>{seriesName()}</span>}>
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
                </>
              )}
            </Show>
            <Show when={props.seriesName && props.episodeCode}>
              <span aria-hidden="true">·</span>
            </Show>
            <Show when={props.episodeCode}>{(episodeCode) => <span>{episodeCode()}</span>}</Show>
          </p>
        </Show>

        <Show when={props.genres.length > 0}>
          <p class={styles.genres}>
            <For each={props.genres}>
              {(genre, index) => (
                <>
                  <Show when={index() > 0}>
                    <span class={styles.genreSeparator} aria-hidden="true">
                      •
                    </span>
                  </Show>
                  <span>{genre}</span>
                </>
              )}
            </For>
          </p>
        </Show>

        <div class={styles.actions}>{props.actions}</div>

        <div class={styles.infoGrid}>
          <Show when={props.overview} fallback={<div />}>
            {(overview) => <p class={styles.overview}>{overview()}</p>}
          </Show>
          <dl class={styles.infoPanel}>
            <For each={props.infoRows}>
              {(row) => (
                <div class={styles.infoItem}>
                  <dt class={styles.infoLabel}>{row.label}</dt>
                  <dd class={styles.infoValue}>{row.value}</dd>
                </div>
              )}
            </For>
          </dl>
        </div>
      </div>
    </section>
  );
}

export function DetailHeroSkeleton() {
  return (
    <section class={styles.skeletonHero} role="status" aria-label="Loading item detail">
      <div class={styles.skeletonBackdrop} aria-hidden="true" />
      <div class={styles.skeletonContent}>
        <span class={styles.skeletonLabel}>Loading details…</span>
        <div class={styles.skeletonBadge} aria-hidden="true" />
        <div class={styles.skeletonTitle} aria-hidden="true" />
        <div class={styles.skeletonLine} aria-hidden="true" />
        <div class={styles.skeletonActions} aria-hidden="true" />
      </div>
    </section>
  );
}
