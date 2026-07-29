import { cx } from '@styled-system/css';
import { Exit, Match } from 'effect';
import { Check, Clapperboard, Heart, RefreshCw } from 'lucide-solid';
import { For, Show, createSignal, createUniqueId, onCleanup, onMount } from 'solid-js';
import {
  videoHomeAspect,
  videoHomeColumnCount,
  type VideoHomeRowKind,
} from '~utils/videoHomeLayout';

import type {
  VideoHomeItem,
  VideoLibraryKind,
  VideoLibraryPlayedFilter,
  VideoLibrarySort,
  VideoSeason,
  VideoUserDataAction,
  VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
} from '../../bindings';
import { commandFailureMessage } from '../../effects/commands';
import type { CommandError } from '../../effects/errors';
import { Button, Card } from '../ui';
import type { JellyPilotSelectItem } from '../ui';
import { HomeVideoCard } from './HomeVideoCard';
import * as styles from './shared.styles';

export { MediaInfoHoverCard } from './MediaInfoHoverCard';
export { DetailHero, DetailHeroSkeleton } from './DetailHero';
export type { DetailHeroInfoRow } from './DetailHero';
export { HomeVideoCard } from './HomeVideoCard';
export { LibraryVideoCard } from './LibraryVideoCard';

export function LibraryStatusPanel(props: { title: string; description?: string }) {
  const titleId = createUniqueId();
  return (
    <Card as="section" variant="elevated" class={styles.statusCard} aria-labelledby={titleId}>
      <div class={styles.statusContent}>
        <div class={styles.statusIcon}>
          <Clapperboard class={styles.iconMd} />
        </div>
        <div class={styles.statusCopy}>
          <h2 id={titleId} class={styles.statusTitle}>
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

export function VideoHomeRow(props: {
  id: string;
  title: string;
  kind: VideoHomeRowKind;
  items: VideoHomeItem[];
  resumeBusyId?: string | null;
  onResume?: (item: VideoHomeItem) => void;
}) {
  const titleId = `row-${props.id}`;
  const gridId = createUniqueId();
  const [expanded, setExpanded] = createSignal(false);
  const [availableWidth, setAvailableWidth] = createSignal(0);
  let rowElement: HTMLElement | undefined;

  const measure = () => {
    const rowWidth = rowElement?.clientWidth ?? 0;
    const fallbackWidth =
      typeof window !== 'undefined' && window.innerWidth > 0 ? window.innerWidth : 0;
    setAvailableWidth(rowWidth > 0 ? rowWidth : fallbackWidth);
  };

  onMount(() => {
    measure();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      onCleanup(() => window.removeEventListener('resize', measure));
      return;
    }

    const observer = new ResizeObserver(measure);
    if (rowElement) {
      observer.observe(rowElement);
    }
    onCleanup(() => observer.disconnect());
  });

  const aspect = () => videoHomeAspect(props.kind);
  const columns = () => videoHomeColumnCount(aspect(), availableWidth());
  const hasOverflow = () => props.items.length > columns();

  return (
    <Show when={props.items.length > 0}>
      <section ref={rowElement} class={styles.row} aria-labelledby={titleId}>
        <div class={styles.rowHeader}>
          <h2 id={titleId} class={styles.rowTitle}>
            {props.title}
          </h2>
          <Show when={hasOverflow()}>
            <Button
              type="button"
              variant="text"
              class={styles.rowDisclosure}
              aria-controls={gridId}
              aria-expanded={expanded()}
              onClick={() => setExpanded((current) => !current)}
            >
              {expanded() ? 'Show Less' : 'See All'}
            </Button>
          </Show>
        </div>
        <div id={gridId} class={cx(styles.videoGrid[columns()], styles.videoGridGap[aspect()])}>
          <For each={props.items}>
            {(item, index) => (
              <Show when={expanded() || index() < columns()}>
                <HomeVideoCard
                  item={item}
                  rowKind={props.kind}
                  busy={props.resumeBusyId === item.id}
                  resumeDisabled={props.resumeBusyId !== null && props.resumeBusyId !== undefined}
                  onResume={
                    props.kind === 'continueWatching' && props.onResume
                      ? () => props.onResume?.(item)
                      : undefined
                  }
                />
              </Show>
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
                  class={cx(
                    styles.iconSm,
                    props.favorite ? styles.favoriteIconSelected : styles.favoriteIcon,
                  )}
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
                  class={cx(
                    styles.iconSm,
                    props.played ? styles.playedIconSelected : styles.playedIcon,
                  )}
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
