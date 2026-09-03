import { describe, expect, test } from 'bun:test';

import {
  formatCommand,
  lintCommand,
  rustClippyWorkspaceCommands,
  rustFormatCommand,
  scriptTestCommands,
  typecheckCommands,
} from './commands';
import { parseCrates, resolveCrates } from './crates';
import { icedHotCommand, icedRunCommand } from './iced';

describe('shared command builders', () => {
  test('builds formatting, linting, and typecheck invocations', () => {
    expect(formatCommand(true)).toEqual({
      command: 'bun',
      args: ['x', 'oxfmt', '--check', 'package.json', 'scripts/**/*.ts', 'lint-staged.config.mjs'],
    });
    expect(lintCommand(true)).toEqual({
      command: 'bun',
      args: [
        'x',
        'oxlint',
        '--fix',
        '--deny-warnings',
        '--no-error-on-unmatched-pattern',
        'scripts',
      ],
    });
    expect(typecheckCommands()).toEqual([
      { command: 'bun', args: ['x', 'tsc', '--noEmit', '-p', 'scripts'] },
    ]);
    expect(scriptTestCommands()).toEqual([{ command: 'bun', args: ['test', 'scripts'] }]);
  });
  test('builds workspace Rust formatting and clippy invocations', () => {
    const format = rustFormatCommand(true);
    expect(format.command).toBe('cargo');
    expect(format.args.slice(0, 3)).toEqual(['fmt', '--manifest-path', 'Cargo.toml']);
    expect(format.args.filter((argument) => argument === '--package')).toHaveLength(7);
    expect(format.args.slice(-2)).toEqual(['--', '--check']);

    const [clippy] = rustClippyWorkspaceCommands();
    expect(clippy?.command).toBe('cargo');
    expect(clippy?.args.slice(0, 3)).toEqual(['clippy', '--manifest-path', 'Cargo.toml']);
    expect(clippy?.args.filter((argument) => argument === '--package')).toHaveLength(7);
    expect(clippy?.args.slice(-6)).toEqual([
      '--all-targets',
      '--all-features',
      '--no-deps',
      '--',
      '-D',
      'warnings',
    ]);
  });
});

describe('crate routing', () => {
  test('resolves single- and multi-package crate aliases', () => {
    expect(parseCrates(['core', 'mpv'])).toEqual(['core', 'mpv']);
    expect(resolveCrates('core')).toEqual(['jellypilot-core']);
    expect(resolveCrates('iced')).toEqual(['jellypilot-ui', 'jellypilot-iced']);
  });

  test('rejects unknown crate aliases', () => {
    expect(() => parseCrates(['unknown'])).toThrow("Unknown crate 'unknown'.");
  });
});

describe('iced command builders', () => {
  test('builds run commands with optional release and smoke boundaries', () => {
    expect(icedRunCommand(false, false)).toEqual({
      command: 'cargo',
      args: ['run', '--manifest-path', 'Cargo.toml', '--package', 'jellypilot-iced'],
    });
    expect(icedRunCommand(true, true)).toEqual({
      command: 'cargo',
      args: [
        'run',
        '--manifest-path',
        'Cargo.toml',
        '--package',
        'jellypilot-iced',
        '--release',
        '--',
        '--smoke-test',
      ],
    });
  });

  test('builds the hot-reload command', () => {
    expect(icedHotCommand()).toEqual({
      command: 'cargo',
      args: [
        'hot',
        '--manifest-path',
        'Cargo.toml',
        '--package',
        'jellypilot-iced',
        '--features',
        'dev',
      ],
    });
  });
});
