import { HoverCard } from '@components/ui';
import { createQuery } from '@tanstack/solid-query';
import { Exit, Option } from 'effect';
import { Check, Heart, LoaderCircle } from 'lucide-solid';
import { createMemo, createSignal, For, Show } from 'solid-js';
import type { JSX } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import { fetchMediaDetail } from '~effects/library';
import type { MediaDetail } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';

import * as styles from './MediaInfoHoverCard.styles';

// Inlined (instead of importing from ./shared) to avoid a shared.tsx <-> card
// Import cycle. Matches the formatRuntime shape used elsewhere.
function formatRuntime(seconds: Option.Option<number>): Option.Option<string> {
  return Option.match(seconds, {
    onNone: () => Option.none(),
    onSome: (value) => {
      const totalMinutes = Math.round(value / 60);
      const hours = Math.floor(totalMinutes / 60);
      const minutes = totalMinutes % 60;
      return Option.some(hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`);
    },
  });
}

/**
 * Presentational body for the Media info hover-card. Renders the normalized
 * MediaDetail as title, meta, genre pills, overview, resume progress, and
 * played/favorite state. Exported so it can be rendered and asserted directly.
 */
export function MediaInfoContent(props: { detail: MediaDetail }) {
  const meta = () =>
    [
      Option.match(props.detail.productionYear, {
        onNone: () => null,
        onSome: (year) => year.toString(),
      }),
      props.detail.itemType,
      Option.getOrElse(formatRuntime(props.detail.runtimeSeconds), () => null),
    ]
      .filter((part): part is string => part !== null)
      .join(' · ');
  const resumePct = () => Option.getOrElse(props.detail.playedPercentage, () => 0);
  const overviewText = () => Option.getOrElse(props.detail.overview, () => null);

  return (
    <div class={styles.content}>
      <p class={styles.title}>{props.detail.name}</p>
      <Show when={meta()}>
        <p class={styles.meta}>{meta()}</p>
      </Show>
      <Show when={props.detail.genres.length > 0}>
        <div class={styles.genres}>
          <For each={props.detail.genres}>
            {(genre) => <span class={styles.genre}>{genre}</span>}
          </For>
        </div>
      </Show>
      <Show when={overviewText()}>{(overview) => <p class={styles.overview}>{overview()}</p>}</Show>
      <Show when={Option.isSome(props.detail.playedPercentage)}>
        <div>
          <div class={styles.progressTrack}>
            <div class={styles.progressBar} style={{ width: `${resumePct()}%` }} />
          </div>
          <p class={styles.watchedText}>{Math.round(resumePct())}% watched</p>
        </div>
      </Show>
      <Show when={props.detail.played || props.detail.favorite}>
        <div class={styles.states}>
          <Show when={props.detail.played}>
            <span class={`${styles.state} ${styles.played}`}>
              <Check class={styles.icon} /> Played
            </span>
          </Show>
          <Show when={props.detail.favorite}>
            <span class={`${styles.state} ${styles.favorite}`}>
              <Heart class={styles.icon} /> Favorite
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
 * open and cached by Solid Query.
 */
export function MediaInfoHoverCard(props: { id: string; itemType: string; children: JSX.Element }) {
  const [open, setOpen] = createSignal(false);
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const detailQuery = createQuery(() => ({
    queryKey: queryKeys.libraryMediaDetail(sessionKey(), props.itemType, props.id),
    queryFn: () => runExit(fetchMediaDetail(props.id, props.itemType)),
    enabled: open() && isLibrarySessionKeyConnected(sessionKey()),
    staleTime: Infinity,
  }));

  return (
    <HoverCard
      onOpenChange={setOpen}
      content={() => (
        <Show
          when={
            !(detailQuery.isPending || (detailQuery.isFetching && !detailQuery.data)) &&
            detailQuery.data
          }
          fallback={
            <div class={styles.loading}>
              <LoaderCircle class={styles.spinner} />
              <span>Loading…</span>
            </div>
          }
        >
          {(exit) =>
            Exit.match(exit(), {
              onFailure: (cause) => (
                <p class={styles.error}>{commandFailureMessage(cause, 'Could not load detail')}</p>
              ),
              onSuccess: (value) => <MediaInfoContent detail={value} />,
            })
          }
        </Show>
      )}
    >
      {props.children}
    </HoverCard>
  );
}
