import { Effect } from 'effect';

import { command, getSetupCommands, wasmBuildCommand } from './commands';
import { runCommand, runCommands } from './process';

export interface DevelopmentOptions {
  readonly rsdoctor: boolean;
  readonly skipSetup: boolean;
}

export interface TestOptions {
  readonly all: boolean;
  readonly skipSetup: boolean;
  readonly watch: boolean;
}

export const runDev = Effect.fn('task.dev')(function* (options: DevelopmentOptions) {
  if (!options.skipSetup) yield* runCommands(getSetupCommands('dev', options.rsdoctor));
  yield* runCommand(
    options.rsdoctor
      ? command('bun', ['x', 'rsbuild'], { RSDOCTOR: 'true' })
      : command('bun', ['x', 'rsbuild', 'dev']),
  );
});

export const runBuild = Effect.fn('task.build')(function* (options: DevelopmentOptions) {
  if (!options.skipSetup) yield* runCommands(getSetupCommands('build', options.rsdoctor));
  yield* runCommand(
    options.rsdoctor
      ? command('bun', ['x', 'rsbuild', 'build'], { RSDOCTOR: 'true' })
      : command('bun', ['x', 'rsbuild', 'build']),
  );
});

export const runPreview = Effect.fn('task.preview')(() =>
  runCommand(command('bun', ['x', 'rsbuild', 'preview'])).pipe(Effect.asVoid),
);

export const runTest = Effect.fn('task.test')(function* (options: TestOptions) {
  if (!options.skipSetup) yield* runCommand(wasmBuildCommand('--dev'));
  yield* runCommand(command('bun', ['x', 'rstest', ...(options.watch ? ['--watch'] : [])]));
  if (options.all) {
    yield* runCommand(command('cargo', ['test', '--manifest-path', 'Cargo.toml', '--workspace']));
  }
});
