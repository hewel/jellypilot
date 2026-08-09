import { commands, events } from '@bindings';
import type {
  EmbeddedPlayerObservation,
  EmbeddedPlayerState,
  PlaybackControlCommand,
  WebPlaybackCapabilities,
} from '@bindings';
import { Effect } from 'effect';

import { runTauriCommand } from './commands';
import type { CommandError } from './errors';

export type EmbeddedPlayerEffect<T> = Effect.Effect<T, CommandError>;

export const fetchEmbeddedPlayerState: EmbeddedPlayerEffect<EmbeddedPlayerState> = runTauriCommand(
  () => commands.embeddedPlayerGetState(),
);

export function registerEmbeddedPlayerCapabilities(
  capabilities: WebPlaybackCapabilities,
): EmbeddedPlayerEffect<EmbeddedPlayerState> {
  return runTauriCommand(() => commands.embeddedPlayerRegisterCapabilities(capabilities));
}

export function controlEmbeddedPlayer(
  command: PlaybackControlCommand,
): EmbeddedPlayerEffect<EmbeddedPlayerState> {
  return runTauriCommand(() => commands.embeddedPlayerControl(command));
}

export function observeEmbeddedPlayer(
  observation: EmbeddedPlayerObservation,
): EmbeddedPlayerEffect<EmbeddedPlayerState> {
  return runTauriCommand(() => commands.embeddedPlayerObserve(observation));
}

export const playEmbeddedPlayerInMpv: EmbeddedPlayerEffect<void> = runTauriCommand(() =>
  commands.embeddedPlayerPlayInMpv(),
).pipe(Effect.asVoid);

export function listenEmbeddedPlayerChanged(
  onState: (state: EmbeddedPlayerState) => void,
): Promise<() => void> {
  return events.embeddedPlayerChanged.listen((event) => onState(event.payload.state));
}
