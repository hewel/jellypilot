import { Badge, Button, ProgressBar } from '@jellypilot/ui';
import { ArrowLeft } from 'lucide-solid';
import { Show, createEffect, createSignal } from 'solid-js';
import type { JSX } from 'solid-js';
import { imageSource } from '~utils/imageSource';

import * as styles from './DetailHero.css';

export interface DetailHeroProps {
  title: string;
  subtitle: JSX.Element;
  backdropImageId: string | null;
  artworkImageId: string | null;
  artworkAspect: 'poster' | 'landscape';
  typeLabel: string;
  typeIcon: JSX.Element;
  badges: JSX.Element;
  actions: JSX.Element;
  resumeProgress?: number | null;
  backLabel?: string;
  onBack?: () => void;
}

export function DetailHero(props: DetailHeroProps) {
  const heroImageId = () => props.backdropImageId ?? props.artworkImageId;
  const [heroImageFailed, setHeroImageFailed] = createSignal(false);
  const [artworkImageFailed, setArtworkImageFailed] = createSignal(false);
  const progressPercent = () => Math.max(0, Math.min(1, props.resumeProgress ?? 0)) * 100;

  createEffect(() => {
    heroImageId();
    setHeroImageFailed(false);
  });

  createEffect(() => {
    props.artworkImageId;
    setArtworkImageFailed(false);
  });

  return (
    <section class={styles.root}>
      <div class={styles.mediaLayer}>
        <Show
          when={!heroImageFailed() ? heroImageId() : null}
          fallback={<div class={styles.fallbackBackdrop} />}
        >
          {(imageId) => (
            <img
              src={imageSource(imageId())}
              alt=""
              aria-hidden="true"
              class={styles.heroImage}
              onError={() => setHeroImageFailed(true)}
            />
          )}
        </Show>
      </div>
      <div class={styles.scrim} />

      <Show when={props.onBack}>
        <Button
          type="button"
          variant="outline"
          size="sm"
          class={styles.backButton}
          onClick={() => props.onBack?.()}
        >
          <ArrowLeft class={styles.backIcon} />
          {props.backLabel ?? 'Back'}
        </Button>
      </Show>

      <div class={styles.content}>
        <div
          class={`${styles.artwork} ${styles.artworkWidth[props.artworkAspect]} ${styles.artworkAspect[props.artworkAspect]}`}
        >
          <Show
            when={!artworkImageFailed() ? props.artworkImageId : null}
            fallback={
              <div class={styles.artworkFallback}>
                <div class={styles.artworkFallbackIcon}>{props.typeIcon}</div>
                <p class={styles.artworkFallbackTitle}>{props.title}</p>
              </div>
            }
          >
            {(imageId) => (
              <img
                src={imageSource(imageId())}
                alt={`${props.title} artwork`}
                class={styles.artworkImage}
                onError={() => setArtworkImageFailed(true)}
              />
            )}
          </Show>
          <Show when={props.resumeProgress != null}>
            <ProgressBar
              value={progressPercent()}
              label={`${props.title} watch progress`}
              class={styles.progressTrack}
            />
          </Show>
        </div>

        <div class={styles.copy}>
          <Badge tone="neutral">{props.typeLabel}</Badge>
          <div class={styles.titleBlock}>
            <h1 class={styles.title}>{props.title}</h1>
            <p class={styles.subtitle}>{props.subtitle}</p>
          </div>
          <div class={styles.badges}>{props.badges}</div>
          <div class={styles.actions}>{props.actions}</div>
        </div>
      </div>
    </section>
  );
}
