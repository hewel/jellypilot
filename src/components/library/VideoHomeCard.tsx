import { Show } from 'solid-js';
import type { VideoHomeItem } from '../../bindings';

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
    <a
      href={`/library/items/${props.item.id}`}
      class="card-filled block overflow-hidden p-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70 transition-all duration-300 hover:border-primary/50 hover:shadow-brand-glow-sm"
    >
      <div
        class={`${props.aspectClass} border-b border-outline-variant bg-surface-container-lowest/60 overflow-hidden`}
      >
        <Show
          when={props.item.artworkUrl}
          fallback={
            <div class="flex h-full items-center justify-center px-4 text-center text-label-small text-on-surface-variant">
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
      <div class="space-y-1 pt-2 pb-3 px-4">
        <p class="line-clamp-2 text-title-medium">{props.item.name}</p>
        <p class="text-body-small">{episodeLabel()}</p>
      </div>
    </a>
  );
}
