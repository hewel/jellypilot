import { Show } from 'solid-js';

import type { VideoHomeItem } from '../../bindings';
import { CardLink } from '../ui';

export function VideoHomeCard(props: {
  item: VideoHomeItem;
  aspectClass: 'aspect-[2/3]' | 'aspect-video';
}) {
  const episodeLabel = () => {
    if (!props.item.seriesName) {
      return `${props.item.itemType} · ${props.item.productionYear}`;
    }
    const number =
      props.item.seasonNumber && props.item.episodeNumber
        ? `S${props.item.seasonNumber.toString().padStart(2, '0')}E${props.item.episodeNumber.toString().padStart(2, '0')}`
        : props.item.itemType;
    return `${props.item.seriesName} · ${number}`;
  };

  return (
    <CardLink
      variant="filled"
      href={`/library/items/${props.item.id}`}
      aria-label={`Open ${props.item.name}`}
      class="focus-visible:ring-secondary/70 hover:border-primary/50 block overflow-hidden !p-0 transition-all duration-300 hover:shadow-[0_0_10px_rgba(79,70,229,0.35)] focus-visible:ring-2 focus-visible:outline-none"
    >
      <div
        class={`${props.aspectClass} border-outline-variant bg-surface-container-lowest/60 overflow-hidden border-b`}
      >
        <Show
          when={props.item.artworkUrl}
          fallback={
            <div class="text-on-surface-variant flex h-full items-center justify-center px-4 text-center text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
              No artwork
            </div>
          }
        >
          {(artworkUrl) => (
            <img
              src={artworkUrl()}
              alt={`${props.item.name} artwork`}
              class="h-full w-full object-cover"
              loading="lazy"
            />
          )}
        </Show>
      </div>
      <div class="space-y-1 px-4 pt-2 pb-3">
        <p class="text-on-surface line-clamp-2 text-[16px] leading-[24px] font-semibold">
          {props.item.name}
        </p>
        <p class="text-on-surface-variant/80 text-[12px] leading-[16px]">{episodeLabel()}</p>
      </div>
    </CardLink>
  );
}
