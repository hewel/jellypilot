import { expect, test } from '@rstest/core';

import { e2eSetupCommands, getSetupCommands } from '../scripts/task/commands';
import { CRATE_NAMES, resolveCrates } from '../scripts/task/crates';
import { parseCli } from '../scripts/task/parse';

test('maps every task crate short name and rejects unknown crates', () => {
  expect(CRATE_NAMES).toEqual({
    core: ['jellypilot-core'],
    'core-wasm': ['jellypilot-core-wasm'],
    'media-server': ['jellypilot-media-server'],
    mpv: ['jellypilot-mpv'],
    session: ['jellypilot-session'],
    'playback-core': ['jellypilot-playback-core'],
    gtk: ['jellypilot-gtk'],
    iced: ['jellypilot-ui', 'jellypilot-iced'],
  });
  for (const [shortName, packageNames] of Object.entries(CRATE_NAMES)) {
    expect(resolveCrates(shortName)).toBe(packageNames);
  }
  expect(() => resolveCrates('unknown')).toThrow("Unknown crate 'unknown'.");
});

test('parses Rust clippy task variants', () => {
  expect(parseCli(['rust', 'clippy', 'mpv'])).toEqual({
    _tag: 'rust',
    action: 'clippy',
    crates: ['mpv'],
  });
  expect(parseCli(['rust', 'clippy'])).toEqual({
    _tag: 'rust',
    action: 'clippy',
    crates: [],
  });
});

test('parses iced run variants', () => {
  expect(parseCli(['iced', 'run'])).toEqual({
    _tag: 'iced',
    smoke: false,
  });
  expect(parseCli(['iced', 'run', '--smoke'])).toEqual({
    _tag: 'iced',
    smoke: true,
  });
});

test('parses test flags and passes E2E arguments through unchanged', () => {
  expect(parseCli(['test', '--all'])).toEqual({
    _tag: 'test',
    all: true,
    skipSetup: false,
    watch: false,
  });
  expect(parseCli(['e2e', 'test', '--some-flag'])).toEqual({
    _tag: 'e2e',
    action: 'test',
    args: ['--some-flag'],
    skipSetup: false,
  });
});

test('preserves FFmpeg target selection for packaging callers', () => {
  expect(
    parseCli(['ffmpeg', 'prepare', '--verify', '--target', 'x86_64-unknown-linux-gnu']),
  ).toEqual({
    _tag: 'ffmpeg',
    target: 'x86_64-unknown-linux-gnu',
    verify: true,
  });
});

test('returns help for a bare task and rejects unknown commands', () => {
  expect(parseCli([])).toEqual({ _tag: 'help' });
  expect(() => parseCli(['unknown'])).toThrow('Unknown task command: unknown');
});
test('parses panda codegen and restores pre-deletion E2E setup chains', () => {
  expect(parseCli(['panda', 'codegen'])).toEqual({ _tag: 'panda' });
  expect(() => parseCli(['panda'])).toThrow('Missing Panda command.');
  expect(e2eSetupCommands('build').map((request) => request.args)).toEqual([
    ['scripts/prepare-ffmpeg-sidecar.ts'],
    ['scripts/build-library-browse-wasm.ts', '--dev'],
  ]);
  expect(e2eSetupCommands('verify')).toHaveLength(2);
  expect(e2eSetupCommands('typecheck')).toHaveLength(1);
  expect(e2eSetupCommands('test')).toHaveLength(0);
});
test('forwards review parity arguments and rejects contradictory test flags', () => {
  expect(parseCli(['review', 'parity', 'launch'])).toEqual({
    _tag: 'review',
    action: 'parity',
    args: ['launch'],
  });
  expect(parseCli(['review', 'panda-tauri'])).toEqual({
    _tag: 'review',
    action: 'panda-tauri',
    args: [],
  });
  expect(() => parseCli(['test', '--watch', '--all'])).toThrow(
    'Cannot combine --watch with --all.',
  );
});

test('strips e2e --skip-setup from forwarded arguments', () => {
  expect(parseCli(['e2e', 'typecheck', '--skip-setup'])).toEqual({
    _tag: 'e2e',
    action: 'typecheck',
    args: [],
    skipSetup: true,
  });
  expect(parseCli(['e2e', 'test', '--spec', 'x.e2e.ts'])).toEqual({
    _tag: 'e2e',
    action: 'test',
    args: ['--spec', 'x.e2e.ts'],
    skipSetup: false,
  });
});

test('selects the setup WASM mode for dev and build variants', () => {
  expect(getSetupCommands('dev').map((request) => request.args)).toEqual([
    ['scripts/prepare-ffmpeg-sidecar.ts'],
    ['x', 'panda', 'codegen'],
    ['scripts/build-library-browse-wasm.ts', '--dev'],
  ]);
  expect(getSetupCommands('build').at(-1)?.args).toEqual(['scripts/build-library-browse-wasm.ts']);
  expect(getSetupCommands('build', true).at(-1)?.args).toEqual([
    'scripts/build-library-browse-wasm.ts',
    '--release',
  ]);
});
