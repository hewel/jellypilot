import { Exit, Match } from 'effect';
import { Check, Clapperboard, Heart, RefreshCw } from 'lucide-solid';
import { For, Show, createSignal } from 'solid-js';
import type { JSX } from 'solid-js';

import type {
  VideoHomeItem,
  VideoItemDetail,
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
import { Button, Card } from '../ui';
import type { JellyPilotSelectItem } from '../ui';
import { MediaInfoHoverCard } from './MediaInfoHoverCard';
import * as styles from './shared.styles';
import { VideoCard } from './VideoCard';
import type { VideoCardAspectClass } from './VideoCard';

export function LibraryStatusPanel(props: { title: string; description?: string }) {
  return (
    <Card
      as="section"
      variant="elevated"
      class={styles.statusCard}
      aria-labelledby="video-home-status-title"
    >
      <div class={styles.statusContent}>
        <div class={styles.statusIcon}>
          <Clapperboard class={styles.iconMd} />
        </div>
        <div class={styles.statusCopy}>
          <h2 id="video-home-status-title" class={styles.statusTitle}>
            {props.title}
          </h2>
          <p class={styles.statusDescription}>
            {props.description ??
              'JellyPilot is checking the current Jellyfin session before loading Library data.'}
          </p>
        </div>
      </div>
    </Card>
  );
}

type VideoHomeRowKind = 'continueWatching' | 'nextUp' | 'latestMovies' | 'latestEpisodes';

const videoHomeAspectClass = (kind: VideoHomeRowKind): VideoCardAspectClass =>
  kind === 'latestMovies' ? 'poster' : 'video';

export function VideoHomeRow(props: {
  id: string;
  title: string;
  kind: VideoHomeRowKind;
  items: VideoHomeItem[];
}) {
  return (
    <Show when={props.items.length > 0}>
      <section class={styles.row} aria-labelledby={`row-${props.id}`}>
        <h2 id={`row-${props.id}`} class={styles.rowTitle}>
          {props.title}
        </h2>
        <div class={styles.videoGrid}>
          <For each={props.items}>
            {(item) => (
              <MediaInfoHoverCard id={item.id} itemType={item.itemType}>
                <VideoCard kind="home" item={item} aspectClass={videoHomeAspectClass(props.kind)} />
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

export const playedFilterLabel = Match.type<VideoLibraryPlayedFilter>().pipe(
  Match.withReturnType<string>(),
  Match.when('played', () => 'Played'),
  Match.when('unplayed', () => 'Unplayed'),
  Match.orElse(() => 'All'),
);

export const sortItems: JellyPilotSelectItem<VideoLibrarySort>[] = [
  { label: 'Title', value: 'title' },
  { label: 'Recently added', value: 'recentlyAdded' },
  { label: 'Release date', value: 'releaseDate' },
];

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

export function detailSubtitleElement(detail: VideoItemDetail): JSX.Element {
  if (detail.itemType === 'Episode' && detail.seriesName) {
    const episode =
      detail.seasonNumber !== null && detail.episodeNumber !== null
        ? `S${detail.seasonNumber.toString().padStart(2, '0')}E${detail.episodeNumber.toString().padStart(2, '0')}`
        : 'Episode';

    return (
      <>
        <Show when={detail.seriesId} fallback={<span>{detail.seriesName}</span>}>
          {(seriesId) => (
            <a href={`/library/shows/${seriesId()}`} class={styles.subtitleLink}>
              {detail.seriesName}
            </a>
          )}
        </Show>
        {' · '}
        {episode}
      </>
    );
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
    <div class={styles.userDataControls}>
      <div class={styles.userDataActions}>
        <Button
          type="button"
          variant="secondary"
          class={styles.pillButton}
          classList={{ [styles.favoriteSelected]: props.favorite }}
          disabled={busy() !== null}
          onClick={() => void runAction(favoriteAction())}
          leadingIcon={
            <Show
              when={busy() === favoriteAction()}
              fallback={
                <Heart
                  class={`${styles.iconSm} ${props.favorite ? styles.favoriteIconSelected : styles.favoriteIcon}`}
                />
              }
            >
              <RefreshCw class={styles.spinIcon} />
            </Show>
          }
        >
          {busy() === favoriteAction() ? 'Updating...' : props.favorite ? 'Unfavorite' : 'Favorite'}
        </Button>
        <Button
          type="button"
          variant="secondary"
          class={styles.pillButton}
          classList={{ [styles.playedSelected]: props.played }}
          disabled={busy() !== null}
          onClick={() => void runAction(playedAction())}
          leadingIcon={
            <Show
              when={busy() === playedAction()}
              fallback={
                <Check
                  class={`${styles.iconSm} ${props.played ? styles.playedIconSelected : styles.playedIcon}`}
                />
              }
            >
              <RefreshCw class={styles.spinIcon} />
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
      <Show when={error()}>{(message) => <p class={styles.errorText}>{message()}</p>}</Show>
    </div>
  );
}
