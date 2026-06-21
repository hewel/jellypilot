import { HoverCard } from '@ark-ui/solid/hover-card';
import { Exit } from 'effect';
import { Check, Heart, LoaderCircle } from 'lucide-solid';
import { createResource, createSignal, For, Show } from 'solid-js';
import type { JSX } from 'solid-js';
import { Portal } from 'solid-js/web';

import { commandFailureMessage } from '../../effects/commands';
import { fetchMediaDetail } from '../../effects/library';
import type { MediaDetail } from '../../effects/library';

// Inlined (instead of importing from ./shared) to avoid a shared.tsx <-> card
// Import cycle. Matches the formatRuntime shape used elsewhere.
function formatRuntime(seconds: number | null): string | null {
  if (seconds === null) {
    return null;
  }
  const totalMinutes = Math.round(seconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

/**
 * Presentational body for the Media info hover-card. Renders the normalized
 * MediaDetail as title, meta, genre pills, overview, resume progress, and
 * played/favorite state. Exported so it can be rendered and asserted directly.
 */
export function MediaInfoContent(props: { detail: MediaDetail }) {
  const meta = () =>
    [
      props.detail.productionYear?.toString() ?? null,
      props.detail.itemType,
      formatRuntime(props.detail.runtimeSeconds),
    ]
      .filter((part): part is string => part !== null)
      .join(' · ');
  const resumePct = () => props.detail.playedPercentage ?? 0;

  return (
    <div class="space-y-2">
      <p class="text-on-surface line-clamp-2 text-[14px] leading-[20px] font-semibold">
        {props.detail.name}
      </p>
      <Show when={meta()}>
        <p class="text-on-surface-variant text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
          {meta()}
        </p>
      </Show>
      <Show when={props.detail.genres.length > 0}>
        <div class="flex flex-wrap gap-1">
          <For each={props.detail.genres}>
            {(genre) => (
              <span class="bg-surface-container-highest/70 text-on-surface-variant rounded-full px-2 py-0.5 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                {genre}
              </span>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.detail.overview}>
        {(overview) => (
          <p class="text-on-surface-variant/90 line-clamp-3 text-[12px] leading-[16px]">
            {overview()}
          </p>
        )}
      </Show>
      <Show when={props.detail.playedPercentage !== null}>
        <div>
          <div class="bg-surface-container-highest/70 h-1 w-full overflow-hidden rounded-full">
            <div class="bg-secondary h-full" style={{ width: `${resumePct()}%` }} />
          </div>
          <p class="text-on-surface-variant mt-1 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
            {Math.round(resumePct())}% watched
          </p>
        </div>
      </Show>
      <Show when={props.detail.played || props.detail.favorite}>
        <div class="text-on-surface-variant flex flex-wrap gap-3 pt-0.5 text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
          <Show when={props.detail.played}>
            <span class="text-tertiary flex items-center gap-1">
              <Check class="h-3.5 w-3.5" /> Played
            </span>
          </Show>
          <Show when={props.detail.favorite}>
            <span class="text-secondary flex items-center gap-1">
              <Heart class="h-3.5 w-3.5" /> Favorite
            </span>
          </Show>
        </div>
      </Show>
    </div>
  );
}

/**
 * Wraps a media card so hovering it reveals a popover with the item's full
 * detail (overview, genres, runtime, resume, user-data state). The card is
 * rendered untouched inside the hover-card trigger; detail is fetched on first
 * open and cached per item id.
 */
export function MediaInfoHoverCard(props: { id: string; itemType: string; children: JSX.Element }) {
  const [open, setOpen] = createSignal(false);
  const [detail] = createResource(
    () => (open() ? props.id : null),
    (id) => fetchMediaDetail(id, props.itemType),
  );

  return (
    <HoverCard.Root
      openDelay={500}
      closeDelay={150}
      unmountOnExit
      positioning={{ gutter: 10, placement: 'top' }}
      onOpenChange={(details) => setOpen(details.open)}
    >
      <HoverCard.Trigger
        asChild={(triggerProps) => <div {...triggerProps()}>{props.children}</div>}
      />
      <Portal>
        <HoverCard.Positioner>
          <HoverCard.Content class="border-outline-variant bg-surface-container-lowest z-100 w-80 max-w-[min(90vw,24rem)] rounded-2xl border p-4 shadow-2xl backdrop-blur-md">
            <Show
              when={detail.state !== 'pending' && detail()}
              fallback={
                <div class="text-on-surface-variant flex items-center justify-center gap-2 py-3 text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                  <LoaderCircle class="h-4 w-4 animate-spin" />
                  <span>Loading…</span>
                </div>
              }
            >
              {(exit) =>
                Exit.match(exit(), {
                  onFailure: (cause) => (
                    <p class="text-error/90 py-2 text-center text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase">
                      {commandFailureMessage(cause, 'Could not load detail')}
                    </p>
                  ),
                  onSuccess: (value) => <MediaInfoContent detail={value} />,
                })
              }
            </Show>
          </HoverCard.Content>
        </HoverCard.Positioner>
      </Portal>
    </HoverCard.Root>
  );
}
