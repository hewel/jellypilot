import { commands } from '@bindings';
import { createEffect, createMemo, createSignal, Show, type JSX } from 'solid-js';
import { imageSource } from '~utils/imageSource';

export interface LibraryImageProps {
  imageId: string | null;
  alt: string;
  class?: string;
  loading?: 'lazy' | 'eager';
  'aria-hidden'?: boolean;
  fallback?: JSX.Element;
}

/**
 * Shared image component with AVIF rejection recovery.
 * On first error, reports the AVIF rejection to the backend and retries once.
 * On second error, renders the fallback (or nothing if no fallback provided).
 */
export function LibraryImage(props: LibraryImageProps) {
  const [attempt, setAttempt] = createSignal(0);
  const [failed, setFailed] = createSignal(false);

  const baseUrl = createMemo(() => {
    const id = props.imageId;
    return id ? imageSource(id) : '';
  });

  const src = createMemo(() => {
    const base = baseUrl();
    if (!base) return '';
    const retry = attempt();
    return retry > 0 ? `${base}?retry=${retry}` : base;
  });

  // Reset when imageId changes.
  createEffect(() => {
    baseUrl();
    setAttempt(0);
    setFailed(false);
  });

  const handleError = () => {
    const currentAttempt = attempt();
    if (currentAttempt === 0) {
      // First failure: report AVIF rejection (fire-and-forget) and retry once.
      const imageId = props.imageId;
      if (imageId) {
        commands.imageRejectAvif(imageId).catch(() => {});
      }
      setAttempt(1);
    } else {
      // Second failure: give up.
      setFailed(true);
    }
  };

  return (
    <Show when={!failed() && Boolean(baseUrl())} fallback={props.fallback}>
      <img
        src={src()}
        alt={props.alt}
        class={props.class}
        loading={props.loading}
        aria-hidden={props['aria-hidden']}
        onError={handleError}
      />
    </Show>
  );
}
