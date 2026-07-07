import { commands } from '@bindings';
import type { AppConfig, ThemePreference } from '@bindings';
import { Effect, Option } from 'effect';

import { runTauriCommand, runTauriCommandRaw } from './commands';
import type { CommandError } from './errors';

/** Detect MPV executable path. Returns Some(path) when found, None otherwise. */
export function detectMpv(): Effect.Effect<Option.Option<string>, CommandError> {
  return runTauriCommandRaw(() => commands.configDetectMpv()).pipe(
    Effect.map(Option.fromNullishOr),
  );
}

export function fetchConfig(): Effect.Effect<AppConfig, CommandError> {
  return runTauriCommandRaw(() => commands.configGet());
}

export function saveConfig(config: AppConfig): Effect.Effect<void, CommandError> {
  return runTauriCommand(() => commands.configSet(config)).pipe(Effect.asVoid);
}

export function saveThemePreference(
  themePreference: ThemePreference,
): Effect.Effect<void, CommandError> {
  return fetchConfig().pipe(
    Effect.flatMap((config) =>
      saveConfig({
        ...config,
        themePreference,
      }),
    ),
  );
}
