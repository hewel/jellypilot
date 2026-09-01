import { Effect } from 'effect';

import { command, type CommandSpec } from './commands';
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

export const runIced = Effect.fn('task.iced')((smoke: boolean, release: boolean) =>
  runCommand(icedRunCommand(smoke, release)).pipe(Effect.asVoid),
);
export const runHot = Effect.fn('task.hot')(() => runCommand(icedHotCommand()).pipe(Effect.asVoid));
