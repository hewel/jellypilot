import { Effect } from 'effect';

import { command, type CommandSpec } from './commands';
import type { TaskProcessError } from './errors';
import { runCommand } from './process';

export function icedRunCommand(smoke: boolean, release: boolean): CommandSpec {
  return command('cargo', [
    'run',
    '--manifest-path',
    'Cargo.toml',
    '--package',
    'jellypilot-iced',
    ...(release ? ['--release'] : []),
    ...(smoke ? ['--', '--smoke-test'] : []),
  ]);
}

export function icedHotCommand(): CommandSpec {
  return command('cargo', [
    'hot',
    '--manifest-path',
    'Cargo.toml',
    '--package',
    'jellypilot-iced',
    '--features',
    'dev',
  ]);
}

// Mirrors the `=== step [FAILED] ===` convention from check.ts so a failing
// smoke gate ends with an attributable segment instead of a bare error dump.
const printSmokeFailure = (error: TaskProcessError): Effect.Effect<void> =>
  Effect.sync(() => {
    console.error('\n=== iced smoke [FAILED] ===');
    console.error(`command: ${error.command}`);
    console.error(`exit: ${error.exitCode ?? 'signal'}`);
    console.error(
      'hint: cargo compile output precedes app logs in the stream; app tracing writes to stderr via JELLYPILOT_LOG (default warn) — re-run with JELLYPILOT_LOG=debug bun run task iced run --smoke',
    );
  });

export const runIced = Effect.fn('task.iced')((smoke: boolean, release: boolean) =>
  runCommand(icedRunCommand(smoke, release)).pipe(
    smoke
      ? Effect.catchTag('TaskProcessError', (error: TaskProcessError) =>
          Effect.gen(function* () {
            yield* printSmokeFailure(error);
            return yield* Effect.fail(error);
          }),
        )
      : (effect) => effect,
    Effect.asVoid,
  ),
);
export const runHot = Effect.fn('task.hot')(() => runCommand(icedHotCommand()).pipe(Effect.asVoid));
