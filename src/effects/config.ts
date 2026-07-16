import { commands } from '@bindings';
import type { AppConfig } from '@bindings';
import { Effect, Option } from 'effect';

import { runTauriCommand, runTauriCommandRaw } from './commands';
import type { CommandError } from './errors';

/** Detect MPV executable path. Returns Some(path) when found, None otherwise. */
export const detectMpv: Effect.Effect<Option.Option<string>, CommandError> = runTauriCommandRaw(
  () => commands.configDetectMpv(),
).pipe(Effect.map(Option.fromNullishOr));

export const fetchConfig: Effect.Effect<AppConfig, CommandError> = runTauriCommandRaw(() =>
  commands.configGet(),
);

export function saveConfig(config: AppConfig): Effect.Effect<void, CommandError> {
  return runTauriCommand(() => commands.configSet(config)).pipe(Effect.asVoid);
}
