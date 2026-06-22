import { Cause, Effect, Option } from 'effect';

import type { CommandError } from '../bindings';
import { CommandError as CommandErrorTag } from './errors';

/**
 * Wrap a Tauri command that returns the generated specta result shape.
 * Converts `{ status: 'error' }` into a typed `CommandError` failure.
 */
export function runTauriCommand<T>(
  command: () => Promise<{ status: 'ok'; data: T } | { status: 'error'; error: CommandError }>,
): Effect.Effect<T, CommandErrorTag> {
  return Effect.tryPromise({
    catch: (error) => {
      if (error instanceof CommandErrorTag) return error;
      return new CommandErrorTag({
        message: error instanceof Error ? error.message : 'Command failed',
      });
    },
    try: async () => {
      const result = await command();
      if (result.status === 'error') {
        throw new CommandErrorTag({
          code: result.error.code,
          message: result.error.message,
        });
      }
      return result.data;
    },
  });
}
export function commandFailure(
  cause: Cause.Cause<CommandErrorTag>,
): Option.Option<CommandErrorTag> {
  for (const reason of cause.reasons) {
    if (Cause.isFailReason(reason)) {
      return Option.some(reason.error);
    }
  }
  return Option.none();
}

export function commandFailureMessage(
  cause: Cause.Cause<CommandErrorTag>,
  fallback: string,
): string {
  return Option.match(commandFailure(cause), {
    onNone: () => fallback,
    onSome: (error) => error.message || fallback,
  });
}

/**
 * Wrap a Tauri command that returns a plain value (no specta status wrapper).
 */
export function runTauriCommandRaw<T>(
  command: () => Promise<T>,
): Effect.Effect<T, CommandErrorTag> {
  return Effect.tryPromise({
    catch: (error) =>
      new CommandErrorTag({
        message: error instanceof Error ? error.message : 'Command failed',
      }),
    try: () => command(),
  });
}
