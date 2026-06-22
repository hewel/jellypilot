import { commands, events } from '@bindings';
import type { NowPlayingState, PropertyValue } from '@bindings';
import { Effect, Exit, Option } from 'effect';

import { runTauriCommand } from './commands';
import type { CommandError } from './errors';

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
    Effect.try({
      try: () => JSON.parse(value) as unknown,
      catch: (cause) => cause,
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

export function fetchNowPlayingState(): NowPlayingEffect<NowPlayingState> {
  return runTauriCommand(() => commands.nowPlayingGetState());
}

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

export function playPreviousEpisode(): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.jellyfinPlayPreviousEpisode()).pipe(Effect.asVoid);
}

export function setPause(paused: boolean): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvSetPause(paused)).pipe(Effect.asVoid);
}

export function stopMpv(): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvStop()).pipe(Effect.asVoid);
}

export function playNextEpisode(): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.jellyfinPlayNextEpisode()).pipe(Effect.asVoid);
}

export function startMpv(): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvStart()).pipe(Effect.asVoid);
}

export function toggleMute(): NowPlayingEffect<void> {
  return runTauriCommand(() => commands.mpvToggleMute()).pipe(Effect.asVoid);
}

export function listenNowPlayingChanged(
  onState: (state: NowPlayingState) => void,
): Promise<() => void> {
  return events.nowPlayingChanged.listen((event) => onState(event.payload.state));
}
