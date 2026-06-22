import { Exit } from 'effect';
import { Check, Clapperboard, Film, Heart, RefreshCw, Tv } from 'lucide-solid';
import { For, Show, createSignal } from 'solid-js';

import type {
  VideoHomeItem,
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
  VideoSeason,
  VideoShowDetail,
  VideoUserDataAction,
  VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
} from '../../bindings';
import { commandFailureMessage } from '../../effects/commands';
import type { CommandError } from '../../effects/errors';
import { Button, Card, CardLink } from '../ui';
import type { JellyPilotSelectItem } from '../ui';
import { MediaInfoHoverCard } from './MediaInfoHoverCard';
import { VideoHomeCard } from './VideoHomeCard';

export { MediaInfoHoverCard } from './MediaInfoHoverCard';
export { VideoHomeCard } from './VideoHomeCard';

export function LibraryStatusPanel(props: { title: string; description?: string }) {
  return (
    <Card
      as="section"
      variant="elevated"
      class="space-y-5"
      aria-labelledby="video-home-status-title"
    >
      <div class="flex items-start gap-4">
        <div class="border-tertiary/30 bg-tertiary-container/25 text-tertiary flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl border">
          <Clapperboard class="h-6 w-6" />
        </div>
        <div class="space-y-2">
          <h2
            id="video-home-status-title"
            class="font-display text-[24px] leading-[32px] font-bold tracking-tight"
          >
            {props.title}
          </h2>
          <p class="text-on-surface-variant text-[14px] leading-[20px]">
            {props.description ??
              'JellyPilot is checking the current Jellyfin session before loading Library data.'}
          </p>
        </div>
      </div>
    </Card>
  );
}

type VideoHomeRowKind = 'continueWatching' | 'nextUp' | 'latestMovies' | 'latestEpisodes';

type VideoHomeAspectClass = 'aspect-[2/3]' | 'aspect-video';

const videoHomeAspectClass = (kind: VideoHomeRowKind): VideoHomeAspectClass =>
  kind === 'latestMovies' ? 'aspect-[2/3]' : 'aspect-video';

export function VideoHomeRow(props: {
  id: string;
  title: string;
  kind: VideoHomeRowKind;
  items: VideoHomeItem[];
}) {
  return (
    <Show when={props.items.length > 0}>
      <section class="space-y-3" aria-labelledby={`row-${props.id}`}>
        <h2 id={`row-${props.id}`} class="text-on-surface text-[22px] leading-[28px] font-bold">
          {props.title}
        </h2>
        <div class="grid gap-3 sm:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-6">
          <For each={props.items}>
            {(item) => (
              <MediaInfoHoverCard id={item.id} itemType={item.itemType}>
                <VideoHomeCard item={item} aspectClass={videoHomeAspectClass(props.kind)} />
              </MediaInfoHoverCard>
            )}
          </For>
        </div>
      </section>
    </Show>
  );
}

export function libraryTitle(collectionType: VideoLibraryKind) {
  return collectionType === 'tvshows' ? 'Shows' : 'Movies';
}

export function playedFilterLabel(filter: VideoLibraryPlayedFilter) {
  switch (filter) {
    case 'played': {
      return 'Played';
    }
    case 'unplayed': {
      return 'Unplayed';
    }
    default: {
      return 'All';
    }
  }
}

export const sortItems: JellyPilotSelectItem<VideoLibrarySort>[] = [
  { label: 'Title', value: 'title' },
  { label: 'Recently added', value: 'recentlyAdded' },
  { label: 'Release date', value: 'releaseDate' },
];

export function VideoLibraryCard(props: {
  item: VideoLibraryItem;
  collectionType?: VideoLibraryKind;
}) {
  const Icon = props.collectionType === 'tvshows' || props.item.itemType === 'Series' ? Tv : Film;
  const href = () =>
    props.item.itemType === 'Series'
      ? `/library/shows/${props.item.id}`
      : `/library/items/${props.item.id}`;
  const subtitle = () => {
    const year = props.item.productionYear
      ? props.item.productionYear.toString()
      : props.item.itemType;
    const state = props.item.played ? 'Played' : 'Unplayed';
    return `${year} · ${state}`;
  };

  const isPoster = () =>
    props.collectionType === 'tvshows' ||
    props.item.itemType === 'Series' ||
    props.item.itemType === 'Movie';

  const aspectClass = () => (isPoster() ? 'aspect-[2/3]' : 'aspect-video');

  return (
    <CardLink
      variant="filled"
      href={href()}
      aria-label={`Open ${props.item.name}`}
      class="group focus-visible:ring-secondary/70 hover:border-primary/50 block overflow-hidden !p-0 transition-[border-color,box-shadow,transform] duration-300 hover:shadow-[0_0_10px_rgba(79,70,229,0.35)] focus-visible:ring-2 focus-visible:outline-none active:scale-[0.96]"
    >
      <div
        class={`${aspectClass()} border-outline-variant bg-surface-container-lowest/60 overflow-hidden border-b`}
      >
        <Show
          when={props.item.artworkUrl}
          fallback={
            <div class="text-on-surface-variant flex h-full flex-col items-center justify-center gap-2 px-4 text-center text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
              <Icon class="h-5 w-5" />
              <span>No artwork</span>
            </div>
          }
        >
          {(artworkUrl) => (
            <img
              src={artworkUrl()}
              alt={`${props.item.name} artwork`}
              class="h-full w-full object-cover outline outline-1 -outline-offset-1 outline-white/10 transition-transform duration-300 group-hover:scale-105"
              loading="lazy"
            />
          )}
        </Show>
      </div>
      <div class="space-y-2 p-4">
        <p class="text-on-surface line-clamp-2 text-[16px] leading-[24px] font-semibold">
          {props.item.name}
        </p>
        <p class="text-on-surface-variant/80 text-[12px] leading-[16px]">{subtitle()}</p>
        <Show when={props.item.favorite}>
          <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
            Favorite
          </p>
        </Show>
      </div>
    </CardLink>
  );
}

export function formatRuntime(seconds: number | null) {
  if (seconds === null) {
    return null;
  }
  const totalMinutes = Math.round(seconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

export function detailSubtitle(detail: VideoItemDetail) {
  if (detail.itemType === 'Episode' && detail.seriesName) {
    const episode =
      detail.seasonNumber !== null && detail.episodeNumber !== null
        ? `S${detail.seasonNumber.toString().padStart(2, '0')}E${detail.episodeNumber.toString().padStart(2, '0')}`
        : 'Episode';
    return `${detail.seriesName} · ${episode}`;
  }
  return detail.productionYear?.toString() ?? detail.itemType;
}

export function showSubtitle(detail: VideoShowDetail) {
  return detail.productionYear?.toString() ?? 'Series';
}

export function seasonLabel(season: VideoSeason) {
  return season.seasonNumber !== null ? `Season ${season.seasonNumber}` : season.name;
}

export function UserDataControls(props: {
  itemId: string;
  played: boolean;
  favorite: boolean;
  subject: string;
  onUpdate: (
    request: VideoUserDataUpdateRequest,
  ) => Promise<Exit.Exit<VideoUserDataUpdate, CommandError>>;
  onSuccess: () => void;
}) {
  const [busy, setBusy] = createSignal<VideoUserDataAction | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const runAction = async (action: VideoUserDataAction) => {
    if (busy()) {
      return;
    }

    setBusy(action);
    setError(null);
    const result = await props.onUpdate({
      action,
      itemId: props.itemId,
    });
    const message = Exit.match(result, {
      onFailure: (cause) => commandFailureMessage(cause, 'Could not update user data'),
      onSuccess: () => null,
    });
    setError(message);
    setBusy(null);
    if (!message) {
      props.onSuccess();
    }
  };
  const favoriteAction = () => (props.favorite ? 'unfavorite' : 'favorite');
  const playedAction = () => (props.played ? 'markUnplayed' : 'markPlayed');

  return (
    <div class="space-y-2">
      <div class="flex flex-wrap gap-3">
        <Button
          type="button"
          variant="secondary"
          class={`rounded-full ${props.favorite ? 'border-error/30' : ''}`}
          disabled={busy() !== null}
          onClick={() => void runAction(favoriteAction())}
          leadingIcon={
            <Show
              when={busy() === favoriteAction()}
              fallback={
                <Heart
                  class={`h-4 w-4 ${props.favorite ? 'fill-error text-error' : 'text-on-surface-variant'}`}
                />
              }
            >
              <RefreshCw class="text-secondary h-4 w-4 animate-spin" />
            </Show>
          }
        >
          {busy() === favoriteAction() ? 'Updating...' : props.favorite ? 'Unfavorite' : 'Favorite'}
        </Button>
        <Button
          type="button"
          variant="secondary"
          class={`rounded-full ${props.played ? 'border-tertiary/30' : ''}`}
          disabled={busy() !== null}
          onClick={() => void runAction(playedAction())}
          leadingIcon={
            <Show
              when={busy() === playedAction()}
              fallback={
                <Check
                  class={`h-4 w-4 ${props.played ? 'text-tertiary font-bold' : 'text-on-surface-variant'}`}
                />
              }
            >
              <RefreshCw class="text-secondary h-4 w-4 animate-spin" />
            </Show>
          }
        >
          {busy() === playedAction()
            ? 'Updating...'
            : props.played
              ? 'Mark unplayed'
              : 'Mark played'}
        </Button>
      </div>
      <Show when={error()}>
        {(message) => <p class="text-error text-[12px] leading-[16px]">{message()}</p>}
      </Show>
    </div>
  );
}
