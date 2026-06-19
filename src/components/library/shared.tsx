import { Exit } from 'effect';
import {
  Check,
  Clapperboard,
  Film,
  Heart,
  Library,
  RefreshCw,
  Tv,
} from 'lucide-solid';
import { createSignal, For, Show } from 'solid-js';
import type {
  VideoHomeItem,
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibraryShortcut,
  VideoLibrarySort,
  VideoSeason,
  VideoShowDetail,
  VideoUserDataAction,
  VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
} from '../../bindings';
import { commandFailureMessage } from '../../effects/commands';
import type { CommandError } from '../../effects/errors';
import { Button, type JmsrSelectItem } from '../ui';
import { MediaInfoHoverCard } from './MediaInfoHoverCard';
import { VideoHomeCard } from './VideoHomeCard';

export { MediaInfoHoverCard } from './MediaInfoHoverCard';
export { VideoHomeCard } from './VideoHomeCard';

export function LibraryStatusPanel(props: {
  title: string;
  description?: string;
}) {
  return (
    <section
      class="card-elevated space-y-5"
      aria-labelledby="video-home-status-title"
    >
      <div class="flex items-start gap-4">
        <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl border border-tertiary/30 bg-tertiary-container/25 text-tertiary">
          <Clapperboard class="h-6 w-6" />
        </div>
        <div class="space-y-2">
          <h2 id="video-home-status-title" class="text-headline-small">
            {props.title}
          </h2>
          <p class="text-body-medium">
            {props.description ??
              'JMSR is checking the current Jellyfin session before loading Library data.'}
          </p>
        </div>
      </div>
    </section>
  );
}

type VideoHomeRowKind =
  | 'continueWatching'
  | 'nextUp'
  | 'latestMovies'
  | 'latestEpisodes';

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
        <h2 id={`row-${props.id}`} class="text-title-large">
          {props.title}
        </h2>
        <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <For each={props.items}>
            {(item) => (
              <MediaInfoHoverCard id={item.id} itemType={item.itemType}>
                <VideoHomeCard
                  item={item}
                  aspectClass={videoHomeAspectClass(props.kind)}
                />
              </MediaInfoHoverCard>
            )}
          </For>
        </div>
      </section>
    </Show>
  );
}

export function LibraryShortcutRow(props: {
  shortcuts: VideoLibraryShortcut[];
  layout?: 'grid' | 'list';
}) {
  const isList = () => props.layout === 'list';
  return (
    <Show when={props.shortcuts.length > 0}>
      <section class="space-y-3" aria-labelledby="library-shortcuts">
        <h2 id="library-shortcuts" class="text-title-large">
          Video Libraries
        </h2>
        <div
          class={isList() ? 'flex flex-col gap-3' : 'grid gap-3 sm:grid-cols-4'}
        >
          <For each={props.shortcuts}>
            {(shortcut) => (
              <a
                href={`/library/${shortcut.collectionType}/${shortcut.id}`}
                class="card-filled flex items-center justify-between gap-4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70"
              >
                <div>
                  <p class="text-title-medium">{shortcut.name}</p>
                  <p class="text-body-small">
                    {shortcut.collectionType === 'tvshows' ? 'Shows' : 'Movies'}{' '}
                    {shortcut.itemCount !== null
                      ? `· ${shortcut.itemCount} items`
                      : ''}
                  </p>
                </div>
                <Library class="h-5 w-5 shrink-0 text-secondary" />
              </a>
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
    case 'played':
      return 'Played';
    case 'unplayed':
      return 'Unplayed';
    default:
      return 'All';
  }
}

export const sortItems: JmsrSelectItem<VideoLibrarySort>[] = [
  { value: 'title', label: 'Title' },
  { value: 'recentlyAdded', label: 'Recently added' },
  { value: 'releaseDate', label: 'Release date' },
];

export function VideoLibraryCard(props: {
  item: VideoLibraryItem;
  collectionType?: VideoLibraryKind;
}) {
  const Icon =
    props.collectionType === 'tvshows' || props.item.itemType === 'Series'
      ? Tv
      : Film;
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
    <a
      href={href()}
      class="card-filled group block overflow-hidden p-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70 transition-all duration-300 hover:border-primary/50 hover:shadow-brand-glow-sm"
    >
      <div
        class={`${aspectClass()} border-b border-outline-variant bg-surface-container-lowest/60 overflow-hidden`}
      >
        <Show
          when={props.item.artworkUrl}
          fallback={
            <div class="flex h-full flex-col items-center justify-center gap-2 px-4 text-center text-label-small text-on-surface-variant">
              <Icon class="h-5 w-5" />
              <span>No artwork</span>
            </div>
          }
        >
          {(artworkUrl) => (
            <img
              src={artworkUrl()}
              alt={`${props.item.name} artwork`}
              class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
              loading="lazy"
            />
          )}
        </Show>
      </div>
      <div class="space-y-2 p-4">
        <p class="line-clamp-2 text-title-medium">{props.item.name}</p>
        <p class="text-body-small">{subtitle()}</p>
        <Show when={props.item.favorite}>
          <p class="text-label-small text-secondary">Favorite</p>
        </Show>
      </div>
    </a>
  );
}

export function formatRuntime(seconds: number | null) {
  if (seconds === null) return null;
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
  return season.seasonNumber !== null
    ? `Season ${season.seasonNumber}`
    : season.name;
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
    if (busy()) return;

    setBusy(action);
    setError(null);
    const result = await props.onUpdate({
      itemId: props.itemId,
      action,
    });
    const message = Exit.match(result, {
      onFailure: (cause) =>
        commandFailureMessage(cause, 'Could not update user data'),
      onSuccess: () => null,
    });
    setError(message);
    setBusy(null);
    if (!message) props.onSuccess();
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
              <RefreshCw class="h-4 w-4 animate-spin text-secondary" />
            </Show>
          }
        >
          {busy() === favoriteAction()
            ? 'Updating...'
            : props.favorite
              ? 'Unfavorite'
              : 'Favorite'}
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
              <RefreshCw class="h-4 w-4 animate-spin text-secondary" />
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
        {(message) => <p class="text-body-small text-error">{message()}</p>}
      </Show>
    </div>
  );
}
