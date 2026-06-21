import { commands } from '@bindings';
import type { ConnectionState } from '@bindings';
import { Effect } from 'effect';

import { runTauriCommand, runTauriCommandRaw } from './commands';
import type { CommandError } from './errors';

export const connection = runTauriCommandRaw(() => commands.jellyfinGetState());

export function fetchConnectionState(): Effect.Effect<ConnectionState, CommandError> {
  return connection;
}

export function disconnectJellyfin(): Effect.Effect<void, CommandError> {
  return runTauriCommand(() => commands.jellyfinDisconnect()).pipe(Effect.asVoid);
}

export function clearJellyfinSession(): Effect.Effect<void, CommandError> {
  return runTauriCommand(() => commands.jellyfinClearSession()).pipe(Effect.asVoid);
}
