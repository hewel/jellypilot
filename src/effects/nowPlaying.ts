import { commands, events } from '@bindings';
import type { NowPlayingState, PropertyValue } from '@bindings';
import { createQuery, useQueryClient } from '@tanstack/solid-query';
import { Effect, Exit, Option } from 'effect';
import { onCleanup, onMount } from 'solid-js';

import { runTauriCommand } from './commands';
import type { CommandError } from './errors';
import { queryKeys, runExit } from './query';

export type NowPlayingEffect<T> = Effect.Effect<T, CommandError>;

export interface MpvTrack {
  id: number;
  type: 'audio' | 'sub';
  label: string;
  selected: boolean;
}

function readStringField(value: Record<string, unknown>, key: string): Option.Option<string> {
  const field = value[key];
  return typeof field === 'string' && field.trim().length > 0
    ? Option.some(field.trim())
    : Option.none();
}

function trackLabel(track: Record<string, unknown>, fallback: string): string {
  return Option.getOrElse(
    readStringField(track, 'title').pipe(
      Option.orElse(() => readStringField(track, 'lang')),
      Option.orElse(() => readStringField(track, 'codec')),
    ),
    () => fallback,
  );
}

export function parseTrackList(value: PropertyValue): MpvTrack[] {
  if (typeof value !== 'string' || value === 'Null') {
    return [];
  }

  const parsed = Effect.runSyncExit(
    Effect.try(() => {
      const parsed: unknown = JSON.parse(value);
      return parsed;
    }),
  );
  if (Exit.isFailure(parsed) || !Array.isArray(parsed.value)) {
    return [];
  }

  return parsed.value.flatMap((track): MpvTrack[] => {
    if (track === null || typeof track !== 'object') {
      return [];
    }
    const value = track as Record<string, unknown>;
    const id = value.id;
    const type = value.type;
    if (typeof id !== 'number' || (type !== 'audio' && type !== 'sub')) {
      return [];
    }
    return [
      {
        id,
        type,
        label: trackLabel(value, `${type === 'audio' ? 'Audio' : 'Subtitle'} ${id}`),
        selected: value.selected === true,
      },
    ];
  });
}

export const fetchNowPlayingState: NowPlayingEffect<NowPlayingState> = runTauriCommand(() =>
  commands.nowPlayingGetState(),
);

export function fetchMpvTrackList(connected: boolean): NowPlayingEffect<MpvTrack[]> {
  if (!connected) {
    return Effect.succeed([]);
  }
  return runTauriCommand(() => commands.mpvGetProperty('track-list')).pipe(
    Effect.map(parseTrackList),
  );
}

export function setAudioTrack(id: number): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvSetAudioTrack(id)).pipe(Effect.asVoid);
}

export function setSubtitleTrack(id: number): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvSetSubtitleTrack(id)).pipe(Effect.asVoid);
}

export function seekPlayback(positionSeconds: number): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvSeek(positionSeconds)).pipe(Effect.asVoid);
}

export function setVolume(volume: number): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvSetVolume(volume)).pipe(Effect.asVoid);
}

export const playPreviousEpisode = runTauriCommand(() =>
  commands.jellyfinPlayPreviousEpisode(),
).pipe(Effect.asVoid);

export function setPause(paused: boolean): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvSetPause(paused)).pipe(Effect.asVoid);
}

export const stopMpv = runTauriCommand(() => commands.mpvStop()).pipe(Effect.asVoid);

export const playNextEpisode = runTauriCommand(() => commands.jellyfinPlayNextEpisode()).pipe(
  Effect.asVoid,
);

export const startMpv = runTauriCommand(() => commands.mpvStart()).pipe(Effect.asVoid);

export const toggleMute = runTauriCommand(() => commands.mpvToggleMute()).pipe(Effect.asVoid);

export function listenNowPlayingChanged(
  onState: (state: NowPlayingState) => void,
): Promise<() => void> {
  return events.nowPlayingChanged.listen((event) => onState(event.payload.state));
}

/**
 * Shared Now Playing state source: one query cache entry plus the Tauri change
 * listener that keeps it fresh. Consumers read `state()`; `onExternalChange`
 * lets owners clear local drafts or invalidate dependent queries on push.
 */
export function createNowPlayingState(options?: {
  onExternalChange?: (state: NowPlayingState) => void;
}) {
  const queryClient = useQueryClient();
  const query = createQuery(() => ({
    queryKey: queryKeys.nowPlayingState,
    queryFn: () => runExit(fetchNowPlayingState),
  }));

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    listenNowPlayingChanged((state) => {
      queryClient.setQueryData(queryKeys.nowPlayingState, Exit.succeed(state));
      options?.onExternalChange?.(state);
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        cleanup = unlisten;
      }
    });

    onCleanup(() => {
      disposed = true;
      cleanup?.();
    });
  });

  const state = () => (query.data && Exit.isSuccess(query.data) ? query.data.value : null);

  return { query, state };
}
