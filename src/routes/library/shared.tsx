import { createListCollection } from '@ark-ui/solid/collection';
import { Clapperboard, Film, Library, Tv } from 'lucide-solid';
import { createSignal, For, Show } from 'solid-js';
import type {
  VideoHomeItem,
  VideoItemDetail,
  VideoLibraryItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibraryShortcut,
  VideoSeason,
  VideoShowDetail,
  VideoUserDataAction,
} from '../../bindings';
import { updateLibraryUserData } from './data';

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

export function VideoHomeRow(props: {
  id: string;
  title: string;
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
            {(item) => <VideoHomeCard item={item} />}
          </For>
        </div>
      </section>
    </Show>
  );
}

export function VideoHomeCard(props: { item: VideoHomeItem }) {
  const episodeLabel = () => {
    if (!props.item.seriesName) return props.item.itemType;
    const number =
      props.item.seasonNumber && props.item.episodeNumber
        ? `S${props.item.seasonNumber.toString().padStart(2, '0')}E${props.item.episodeNumber.toString().padStart(2, '0')}`
        : props.item.itemType;
    return `${props.item.seriesName} · ${number}`;
  };

  return (
    <a
      href={`/library/items/${props.item.id}`}
      class="card-filled group block min-h-56 overflow-hidden p-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70"
    >
      <div class="aspect-video border-b border-outline-variant bg-surface-container-lowest/60">
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
      <div class="space-y-2 p-4">
        <p class="line-clamp-2 text-title-medium">{props.item.name}</p>
        <p class="text-body-small">{episodeLabel()}</p>
        <Show when={props.item.resumePositionSeconds !== null}>
          <p class="text-label-small text-secondary">
            Resume at {Math.floor(props.item.resumePositionSeconds ?? 0)}s
          </p>
        </Show>
      </div>
    </a>
  );
}

export function LibraryShortcutRow(props: {
  shortcuts: VideoLibraryShortcut[];
}) {
  return (
    <Show when={props.shortcuts.length > 0}>
      <section class="space-y-3" aria-labelledby="library-shortcuts">
        <h2 id="library-shortcuts" class="text-title-large">
          Video Libraries
        </h2>
        <div class="grid gap-3 sm:grid-cols-2">
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

export const sortCollection = createListCollection({
  items: [
    { value: 'title', label: 'Title' },
    { value: 'recentlyAdded', label: 'Recently added' },
    { value: 'releaseDate', label: 'Release date' },
  ],
});

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

  return (
    <a
      href={href()}
      class="card-filled group block min-h-56 overflow-hidden p-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70"
    >
      <div class="aspect-video border-b border-outline-variant bg-surface-container-lowest/60">
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
              class="h-full w-full object-cover"
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
  onSuccess: () => void;
}) {
  const [busy, setBusy] = createSignal<VideoUserDataAction | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const runAction = async (action: VideoUserDataAction) => {
    if (busy()) return;

    setBusy(action);
    setError(null);
    const message = await updateLibraryUserData({
      itemId: props.itemId,
      action,
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
        <button
          type="button"
          class="btn-secondary rounded-full"
          disabled={busy() !== null}
          onClick={() => void runAction(favoriteAction())}
        >
          {busy() === favoriteAction()
            ? 'Updating'
            : props.favorite
              ? `Remove ${props.subject} favorite`
              : `Favorite ${props.subject}`}
        </button>
        <button
          type="button"
          class="btn-secondary rounded-full"
          disabled={busy() !== null}
          onClick={() => void runAction(playedAction())}
        >
          {busy() === playedAction()
            ? 'Updating'
            : props.played
              ? `Mark ${props.subject} unplayed`
              : `Mark ${props.subject} played`}
        </button>
      </div>
      <Show when={error()}>
        {(message) => <p class="text-body-small text-error">{message()}</p>}
      </Show>
    </div>
  );
}
