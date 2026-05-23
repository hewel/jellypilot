import { Effect } from 'effect';
import type { CommandError } from '../bindings';
import { CommandError as CommandErrorTag } from './errors';

/**
 * Wrap a Tauri command that returns the generated specta result shape.
 * Converts `{ status: 'error' }` into a typed `CommandError` failure.
 */
export function runTauriCommand<T>(
  command: () => Promise<
    { status: 'ok'; data: T } | { status: 'error'; error: CommandError }
  >,
): Effect.Effect<T, CommandErrorTag> {
  return Effect.tryPromise({
    try: async () => {
      const result = await command();
      if (result.status === 'error') {
        throw new CommandErrorTag({ message: result.error.message });
      }
      return result.data;
    },
    catch: (error) => {
      if (error instanceof CommandErrorTag) return error;
      return new CommandErrorTag({
        message: error instanceof Error ? error.message : 'Command failed',
      });
    },
  });
}

/**
 * Wrap a Tauri command that returns a plain value (no specta status wrapper).
 */
export function runTauriCommandRaw<T>(
  command: () => Promise<T>,
): Effect.Effect<T, CommandErrorTag> {
  return Effect.tryPromise({
    try: () => command(),
    catch: (error) =>
      new CommandErrorTag({
        message: error instanceof Error ? error.message : 'Command failed',
      }),
  });
}
