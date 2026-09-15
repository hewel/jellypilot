import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { Effect, Match } from 'effect';

import {
  command,
  rustClippyWorkspaceCommands,
  rustFormatCommands,
  type CommandSpec,
} from './commands';
import type { CrateShortName } from './crates';
import { CARGO_MANIFESTS, crateWorkspace, resolveCrates, workspacesFor } from './crates';
import { runCommands } from './process';

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
  if (crates.length === 0) {
    return workspacesFor(crates).map((workspace) =>
      command('cargo', ['check', '--manifest-path', CARGO_MANIFESTS[workspace], '--workspace']),
    );
  }
  return workspacesFor(crates).map((workspace) =>
    command('cargo', [
      'check',
      '--manifest-path',
      CARGO_MANIFESTS[workspace],
      ...packageArguments(crates.filter((crate) => crateWorkspace(crate) === workspace)),
    ]),
  );
}

function rustClippyCommands(crates: readonly CrateShortName[]): readonly CommandSpec[] {
  const lintArgs = ['--all-targets', '--all-features', '--no-deps', '--', '-D', 'warnings'];
  if (crates.length === 0) return rustClippyWorkspaceCommands();
  return workspacesFor(crates).map((workspace) =>
    command('cargo', [
      'clippy',
      '--manifest-path',
      CARGO_MANIFESTS[workspace],
      ...packageArguments(crates.filter((crate) => crateWorkspace(crate) === workspace)),
      ...lintArgs,
    ]),
  );
}

function rustTestCommands(crates: readonly CrateShortName[]): readonly CommandSpec[] {
  // App tests exercise real settings mutations; dirs::config_dir honors
  // XDG_CONFIG_HOME, so point it at a scratch directory to keep the
  // developer's own ~/.config/jellypilot/config.json untouched.
  const env = {
    XDG_CONFIG_HOME: mkdtempSync(join(tmpdir(), 'jellypilot-test-config-')),
  };
  if (crates.length === 0) {
    return workspacesFor(crates).map((workspace) =>
      command('cargo', ['test', '--manifest-path', CARGO_MANIFESTS[workspace], '--workspace'], env),
    );
  }
  return crates.flatMap((crate) =>
    resolveCrates(crate).map((packageName) =>
      command(
        'cargo',
        [
          'test',
          '--manifest-path',
          CARGO_MANIFESTS[crateWorkspace(crate)],
          '--package',
          packageName,
          ...(crate === 'mpv' ? ['--features', 'test-utils'] : []),
        ],
        env,
      ),
    ),
  );
}

export const runRust = Effect.fn('task.rust')((task: RustTask) =>
  Match.value(task).pipe(
    Match.when({ action: 'fmt' }, ({ check }) =>
      runCommands(rustFormatCommands(check)).pipe(Effect.asVoid),
    ),
    Match.when({ action: 'check' }, ({ crates }) => runCommands(rustCheckCommands(crates))),
    Match.when({ action: 'clippy' }, ({ crates }) => runCommands(rustClippyCommands(crates))),
    Match.when({ action: 'test' }, ({ crates }) => runCommands(rustTestCommands(crates))),
    Match.exhaustive,
  ),
);
