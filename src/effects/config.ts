import { commands } from '@bindings';
import type { AppConfig } from '@bindings';
import { Effect } from 'effect';

import { runTauriCommand, runTauriCommandRaw } from './commands';
import type { CommandError } from './errors';

/** Detect MPV executable path. Returns the detected path or null. */
export function detectMpv(): Effect.Effect<string | null, CommandError> {
  return runTauriCommandRaw(() => commands.configDetectMpv());
}

export function fetchConfig(): Effect.Effect<AppConfig, CommandError> {
  return runTauriCommandRaw(() => commands.configGet());
}

export function saveConfig(config: AppConfig): Effect.Effect<void, CommandError> {
  return runTauriCommand(() => commands.configSet(config)).pipe(Effect.asVoid);
}
