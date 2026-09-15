import { CARGO_MANIFESTS, workspacePackages, type CargoWorkspace } from './crates';

export interface CommandSpec {
  readonly command: string;
  readonly args: readonly string[];
  readonly env?: Readonly<Record<string, string>>;
}

export const FORMAT_PATHS = ['package.json', 'scripts/**/*.ts', 'lint-staged.config.mjs'];

export const LINT_PATHS = ['scripts'];

const CARGO_WORKSPACES: readonly CargoWorkspace[] = ['shared', 'desktop'];

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

export function rustFormatCommands(check: boolean): readonly CommandSpec[] {
  return CARGO_WORKSPACES.map((workspace) =>
    command('cargo', [
      'fmt',
      '--manifest-path',
      CARGO_MANIFESTS[workspace],
      ...workspacePackages(workspace).flatMap((packageName) => ['--package', packageName]),
      ...(check ? ['--', '--check'] : []),
    ]),
  );
}

export function rustClippyWorkspaceCommands(): readonly CommandSpec[] {
  const lintArgs = ['--all-targets', '--all-features', '--no-deps', '--', '-D', 'warnings'];
  return CARGO_WORKSPACES.map((workspace) =>
    command('cargo', [
      'clippy',
      '--manifest-path',
      CARGO_MANIFESTS[workspace],
      ...workspacePackages(workspace).flatMap((packageName) => ['--package', packageName]),
      ...lintArgs,
    ]),
  );
}

export function typecheckCommands(): readonly CommandSpec[] {
  return [command('bun', ['x', 'tsc', '--noEmit', '-p', 'scripts'])];
}

export function scriptTestCommands(): readonly CommandSpec[] {
  // A bare filter also traverses generated/native source trees before matching.
  return [command('bun', ['test', './scripts'])];
}
