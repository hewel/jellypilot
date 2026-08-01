import { cx } from '@styled-system/css';
import { Show } from 'solid-js';
import type { JSX } from 'solid-js';
import type { VideoHomeAspect } from '~utils/videoHomeLayout';

import { MediaInfoHoverCard } from './MediaInfoHoverCard';
import * as styles from './VideoCard.styles';

export type VideoCardAspectClass = VideoHomeAspect;

export function CardTitle(props: {
  id: string;
  itemType: string;
  class: string;
  children: JSX.Element;
}) {
  return (
    <MediaInfoHoverCard id={props.id} itemType={props.itemType} class={styles.titleHoverTrigger}>
      <p class={props.class}>{props.children}</p>
    </MediaInfoHoverCard>
  );
}

export function VideoCardSkeleton(props: { aspectClass: VideoCardAspectClass; body?: boolean }) {
  return (
    <div class={styles.card} aria-hidden="true">
      <div
        class={cx(styles.artwork, styles.aspect[props.aspectClass], styles.skeleton)}
        data-aspect={props.aspectClass}
      />
      <Show when={props.body === true || props.aspectClass === 'video'}>
        <div class={styles.skeletonBody}>
          <div class={styles.skeletonTitle} />
          <div class={styles.skeletonSubtitle} />
        </div>
      </Show>
    </div>
  );
}
