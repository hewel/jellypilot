import { Effect, Match } from 'effect';

import {
  command,
  rustClippyWorkspaceCommands,
  rustFormatCommand,
  type CommandSpec,
} from './commands';
import type { CrateShortName } from './crates';
import { resolveCrates } from './crates';
import { runCommand, runCommands } from './process';

export type RustTask =
  | { readonly action: 'fmt'; readonly check: boolean; readonly crates: readonly [] }
  | {
      readonly action: 'check' | 'clippy' | 'test';
      readonly crates: readonly CrateShortName[];
    };

function packageArguments(crates: readonly CrateShortName[]): readonly string[] {
  return crates.flatMap((crate) =>
    resolveCrates(crate).flatMap((packageName) => ['--package', packageName]),
  );
}

function rustCheckCommands(crates: readonly CrateShortName[]): readonly CommandSpec[] {
  return [
    command('cargo', [
      'check',
      '--manifest-path',
      'Cargo.toml',
      ...(crates.length === 0 ? ['--workspace'] : packageArguments(crates)),
    ]),
  ];
}

function rustClippyCommands(crates: readonly CrateShortName[]): readonly CommandSpec[] {
  if (crates.length === 0) return rustClippyWorkspaceCommands();
  return [
    command('cargo', [
      'clippy',
      '--manifest-path',
      'Cargo.toml',
      ...packageArguments(crates),
      '--all-targets',
      '--all-features',
      '--no-deps',
      '--',
      '-D',
      'warnings',
    ]),
  ];
}

function rustTestCommands(crates: readonly CrateShortName[]): readonly CommandSpec[] {
  if (crates.length === 0) {
    return [command('cargo', ['test', '--manifest-path', 'Cargo.toml', '--workspace'])];
  }
  return crates.flatMap((crate) =>
    resolveCrates(crate).map((packageName) =>
      command('cargo', [
        'test',
        '--manifest-path',
        'Cargo.toml',
        '--package',
        packageName,
        ...(crate === 'mpv' ? ['--features', 'test-utils'] : []),
      ]),
    ),
  );
}

export const runRust = Effect.fn('task.rust')((task: RustTask) =>
  Match.value(task).pipe(
    Match.when({ action: 'fmt' }, ({ check }) =>
      runCommand(rustFormatCommand(check)).pipe(Effect.asVoid),
    ),
    Match.when({ action: 'check' }, ({ crates }) => runCommands(rustCheckCommands(crates))),
    Match.when({ action: 'clippy' }, ({ crates }) => runCommands(rustClippyCommands(crates))),
    Match.when({ action: 'test' }, ({ crates }) => runCommands(rustTestCommands(crates))),
    Match.exhaustive,
  ),
);
