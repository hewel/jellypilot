import { Match } from 'effect';

import type { E2eSubcommand } from './parse';

export interface CommandSpec {
  readonly command: string;
  readonly args: readonly string[];
  readonly env?: Readonly<Record<string, string>>;
}

export type SetupMode = 'dev' | 'build';

export const FORMAT_PATHS = [
  'package.json',
  'tsconfig.json',
  'tests/tsconfig.json',
  'e2e/tsconfig.json',
  '*.config.{js,json,jsonc,mjs,ts}',
  'src/**/*.{css,js,jsx,json,jsonc,ts,tsx}',
  'tests/**/*.{js,jsx,json,jsonc,ts,tsx}',
  'e2e/**/*.{js,jsx,json,jsonc,ts,tsx}',
  'scripts/**/*.ts',
  'src-tauri/**/*.json',
];

export const LINT_PATHS = [
  'src',
  'tests',
  'e2e',
  'scripts',
  'rsbuild.config.ts',
  'rstest.config.ts',
  'rstest.setup.ts',
  'panda.config.ts',
  'postcss.config.mjs',
  'lint-staged.config.mjs',
];

export const RUST_FORMAT_PACKAGES = [
  'jellypilot',
  'jellypilot-core',
  'jellypilot-core-wasm',
  'jellypilot-media-server',
  'jellypilot-mpv',
  'jellypilot-session',
  'jellypilot-playback-core',
  'jellypilot-gtk',
];

export const RUST_CLIPPY_PACKAGES = RUST_FORMAT_PACKAGES.slice(1);

export function command(
  executable: string,
  args: readonly string[],
  env?: Readonly<Record<string, string>>,
): CommandSpec {
  return env === undefined ? { command: executable, args } : { command: executable, args, env };
}

export function wasmBuildCommand(flag?: '--dev' | '--release'): CommandSpec {
  return command('bun', [
    'scripts/build-library-browse-wasm.ts',
    ...(flag === undefined ? [] : [flag]),
  ]);
}

export function ffmpegPrepareCommand(verify = false): CommandSpec {
  return command('bun', ['scripts/prepare-ffmpeg-sidecar.ts', ...(verify ? ['--verify'] : [])]);
}

export function pandaCodegenCommand(): CommandSpec {
  return command('bun', ['x', 'panda', 'codegen']);
}

export function getSetupCommands(mode: SetupMode, rsdoctor = false): readonly CommandSpec[] {
  const wasmFlag = Match.value(mode).pipe(
    Match.when('dev', (): '--dev' => '--dev'),
    Match.when('build', (): '--release' | undefined => (rsdoctor ? '--release' : undefined)),
    Match.exhaustive,
  );
  return [ffmpegPrepareCommand(), pandaCodegenCommand(), wasmBuildCommand(wasmFlag)];
}
export function e2eSetupCommands(action: E2eSubcommand): readonly CommandSpec[] {
  if (action === 'build' || action === 'verify') {
    return [ffmpegPrepareCommand(), wasmBuildCommand('--dev')];
  }
  if (action === 'typecheck') return [wasmBuildCommand('--dev')];
  return [];
}

export function formatCommand(check: boolean): CommandSpec {
  return command('bun', ['x', 'oxfmt', check ? '--check' : '--write', ...FORMAT_PATHS]);
}

export function lintCommand(fix: boolean): CommandSpec {
  return command('bun', [
    'x',
    'oxlint',
    ...(fix ? ['--fix'] : []),
    '--deny-warnings',
    '--no-error-on-unmatched-pattern',
    ...LINT_PATHS,
  ]);
}

export function rustFormatCommand(check: boolean): CommandSpec {
  return command('cargo', [
    'fmt',
    '--manifest-path',
    'Cargo.toml',
    ...RUST_FORMAT_PACKAGES.flatMap((packageName) => ['--package', packageName]),
    ...(check ? ['--', '--check'] : []),
  ]);
}

export function rustClippyWorkspaceCommands(): readonly CommandSpec[] {
  const lintArgs = ['--all-targets', '--all-features', '--no-deps', '--', '-D', 'warnings'];
  return [
    command('cargo', ['clippy', '--manifest-path', 'src-tauri/Cargo.toml', ...lintArgs]),
    command('cargo', [
      'clippy',
      '--manifest-path',
      'Cargo.toml',
      ...RUST_CLIPPY_PACKAGES.flatMap((packageName) => ['--package', packageName]),
      ...lintArgs,
    ]),
  ];
}

export function typecheckCommands(): readonly CommandSpec[] {
  return [
    command('bun', ['x', 'tsc', '--noEmit']),
    command('bun', ['x', 'tsc', '--noEmit', '-p', 'scripts']),
  ];
}
