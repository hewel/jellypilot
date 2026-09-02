export interface CommandSpec {
  readonly command: string;
  readonly args: readonly string[];
  readonly env?: Readonly<Record<string, string>>;
}

export const FORMAT_PATHS = ['package.json', 'scripts/**/*.ts', 'lint-staged.config.mjs'];

export const LINT_PATHS = ['scripts'];

export const RUST_FORMAT_PACKAGES = [
  'jellypilot-auth',
  'jellypilot-core',
  'jellypilot-media-server',
  'jellypilot-mpv',
  'jellypilot-session',
  'jellypilot-ui',
  'jellypilot-iced',
];

export function command(
  executable: string,
  args: readonly string[],
  env?: Readonly<Record<string, string>>,
): CommandSpec {
  return env === undefined ? { command: executable, args } : { command: executable, args, env };
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
    command('cargo', [
      'clippy',
      '--manifest-path',
      'Cargo.toml',
      ...RUST_FORMAT_PACKAGES.flatMap((packageName) => ['--package', packageName]),
      ...lintArgs,
    ]),
  ];
}

export function typecheckCommands(): readonly CommandSpec[] {
  return [command('bun', ['x', 'tsc', '--noEmit', '-p', 'scripts'])];
}
