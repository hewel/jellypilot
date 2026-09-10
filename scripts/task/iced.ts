import { Effect } from 'effect';

import { command, type CommandSpec } from './commands';
import type { TaskProcessError } from './errors';
import { embeddedMpvEnvironment, embeddedMpvPaths } from './mpv';
import { runCommand } from './process';

export function icedRunCommand(
  smoke: boolean,
  release: boolean,
  embedded = false,
  environment?: Readonly<Record<string, string>>,
): CommandSpec {
  return command(
    'cargo',
    [
      'run',
      '--manifest-path',
      'Cargo.toml',
      '--package',
      'jellypilot-launcher',
      ...(release ? ['--release'] : []),
      ...(smoke || embedded ? ['--'] : []),
      ...(smoke ? ['--smoke-test'] : []),
      ...(embedded ? ['--embedded'] : []),
    ],
    environment,
  );
}

export const buildIced = Effect.fn('task.iced.build')((release: boolean) =>
  runCommand(
    command('cargo', [
      'build',
      '--locked',
      '--manifest-path',
      'Cargo.toml',
      '--package',
      'jellypilot-launcher',
      ...(release ? ['--release'] : []),
    ]),
  ).pipe(Effect.asVoid),
);

export function icedHotCommand(): CommandSpec {
  return command('cargo', [
    'hot',
    '--manifest-path',
    'Cargo.toml',
    '--package',
    'jellypilot-launcher',
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

export const runIced = Effect.fn('task.iced')(function* (
  smoke: boolean,
  release: boolean,
  embedded: boolean,
  environment: Readonly<Record<string, string | undefined>>,
) {
  const childEnvironment = embedded
    ? yield* embeddedMpvEnvironment(environment)
    : embeddedMpvPaths(environment);
  yield* runCommand(icedRunCommand(smoke, release, embedded, childEnvironment)).pipe(
    smoke
      ? Effect.catchTag('TaskProcessError', (error: TaskProcessError) =>
          Effect.gen(function* () {
            yield* printSmokeFailure(error);
            return yield* Effect.fail(error);
          }),
        )
      : (effect) => effect,
    Effect.asVoid,
  );
});
export const runHot = Effect.fn('task.hot')(
  (environment: Readonly<Record<string, string | undefined>>) =>
    runCommand({ ...icedHotCommand(), env: embeddedMpvPaths(environment) }).pipe(Effect.asVoid),
);
