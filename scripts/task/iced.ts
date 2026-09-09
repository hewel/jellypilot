import { Effect } from 'effect';

import { command, type CommandSpec } from './commands';
import type { TaskProcessError } from './errors';
import { embeddedMpvEnvironment, embeddedMpvPaths } from './mpv';
import type { TaskCommand } from './parse';
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

export function icedLocalVideoCommand(
  task: Extract<TaskCommand, { readonly _tag: 'icedLocalVideo' }>,
): CommandSpec {
  const args = [
    task.action,
    '--manifest-path',
    'crates/jellypilot-player/Cargo.toml',
    '--package',
    'jellypilot-local-video',
  ];
  if (task.action !== 'run') args.push('--package', 'jellypilot-player');
  if (task.action === 'fmt') {
    if (task.check) args.push('--', '--check');
  } else {
    args.push('--target-dir', 'target/iced-local-video');
    if (task.action === 'run') {
      if (task.release) args.push('--release');
      args.push('--');
      if (task.smoke) args.push('--smoke-test');
      if (task.file !== null) args.push('--file', task.file);
      if (task.url !== null || task.urlFromEnv) args.push('--url-env');
    } else if (task.action === 'test') {
      if (task.testFilter !== null) args.push(task.testFilter);
    } else {
      args.push('--all-targets');
      if (task.action === 'clippy') args.push('--', '-D', 'warnings');
    }
  }
  return command(
    'cargo',
    args,
    task.action === 'run' && task.url !== null ? { JELLYPILOT_VIDEO_URL: task.url } : undefined,
  );
}

export const runLocalVideo = Effect.fn('task.iced.localVideo')(
  (task: Extract<TaskCommand, { readonly _tag: 'icedLocalVideo' }>) =>
    runCommand(icedLocalVideoCommand(task)).pipe(Effect.asVoid),
);

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
