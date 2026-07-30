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

/** Shared logical Library Image with immediate error fallback. */
export function LibraryImage(props: LibraryImageProps) {
  const [failed, setFailed] = createSignal(false);

  const baseUrl = createMemo(() => {
    const id = props.imageId;
    return id ? imageSource(id) : '';
  });

  // A new image reference or newly available proxy gets one fresh load.
  createEffect(() => {
    baseUrl();
    setFailed(false);
  });

  const handleError = () => {
    setFailed(true);
  };

  return (
    <Show when={!failed() && Boolean(baseUrl())} fallback={props.fallback}>
      <img
        src={baseUrl()}
        alt={props.alt}
        class={props.class}
        loading={props.loading}
        aria-hidden={props['aria-hidden']}
        onError={handleError}
      />
    </Show>
  );
}
