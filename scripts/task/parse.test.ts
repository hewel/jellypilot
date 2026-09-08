import { describe, expect, test } from 'bun:test';

import { icedLocalVideoCommand } from './iced';
import { parseCli } from './parse';

describe('parseCli', () => {
  test('parses supported task commands and options', () => {
    expect(parseCli([])).toEqual({ _tag: 'help' });
    expect(parseCli(['fmt', '--check'])).toEqual({ _tag: 'fmt', check: true });
    expect(parseCli(['lint', '--fix'])).toEqual({ _tag: 'lint', fix: true });
    expect(parseCli(['rust', 'check', 'core', 'iced'])).toEqual({
      _tag: 'rust',
      action: 'check',
      crates: ['core', 'iced'],
    });
    expect(parseCli(['rust', 'fmt', '--check'])).toEqual({
      _tag: 'rust',
      action: 'fmt',
      check: true,
      crates: [],
    });
    expect(parseCli(['iced', 'run', '--smoke', '--release'])).toEqual({
      _tag: 'iced',
      smoke: true,
      release: true,
    });
    expect(parseCli(['iced', 'hot'])).toEqual({ _tag: 'icedHot' });
  });

  test('preserves a local video path as one application argument, not cargo options', () => {
    const file = 'test-videos/中文 clips/movie #1.mp4';
    const task = parseCli(['iced', 'local-video', 'run', '--file', file, '--release', '--smoke']);
    expect(task).toMatchObject({
      _tag: 'icedLocalVideo',
      action: 'run',
      file,
      smoke: true,
      release: true,
    });
    if (task._tag !== 'icedLocalVideo') throw new Error('Expected local video task.');
    const { args } = icedLocalVideoCommand(task);
    expect(args.slice(args.indexOf('--') + 1)).toEqual(['--smoke-test', '--file', file]);
  });

  test('distinguishes local video test filters from cargo options and extra arguments', () => {
    expect(parseCli(['iced', 'local-video', 'test', 'playback::paused_seek'])).toMatchObject({
      _tag: 'icedLocalVideo',
      action: 'test',
      testFilter: 'playback::paused_seek',
    });
    for (const args of [
      ['test', '--release'],
      ['test', '--', '--nocapture'],
      ['test', 'paused_seek', 'replacement'],
      ['check', '--file', 'movie.mp4'],
      ['clippy', '--smoke'],
      ['fmt', '--release'],
      ['run', '--features', 'dev'],
      ['run', 'movie.mp4'],
      [],
      ['hot'],
    ]) {
      expect(() => parseCli(['iced', 'local-video', ...args])).toThrow();
    }
  });

  test('rejects missing, empty, option-like, or duplicate local video file values', () => {
    for (const args of [
      ['--file'],
      ['--file', ''],
      ['--file', '--smoke'],
      ['--file', 'first.mp4', '--file', 'second.mp4'],
    ]) {
      expect(() => parseCli(['iced', 'local-video', 'run', ...args])).toThrow();
    }
  });

  test('parses monitor process options with defaults and explicit overrides', () => {
    expect(
      parseCli(['monitor', '--pid', '123', '--out', 'target/resources/visible.ndjson']),
    ).toEqual({
      _tag: 'monitor',
      pid: 123,
      samples: 301,
      intervalMs: 1000,
      output: 'target/resources/visible.ndjson',
      label: null,
    });
    expect(
      parseCli([
        'monitor',
        '--pid',
        '123',
        '--out',
        'target/resources/v.ndjson',
        '--label',
        'visible run 1',
      ]),
    ).toMatchObject({
      _tag: 'monitor',
      label: 'visible run 1',
    });
    expect(
      parseCli([
        'monitor',
        '--pid',
        '123',
        '--out',
        'target/resources/hidden.ndjson',
        '--samples',
        '2',
        '--interval-ms',
        '50',
      ]),
    ).toEqual({
      _tag: 'monitor',
      pid: 123,
      samples: 2,
      intervalMs: 50,
      output: 'target/resources/hidden.ndjson',
      label: null,
    });
  });

  test('rejects missing commands, unknown options, and unknown crates', () => {
    expect(() => parseCli(['check', 'extra'])).toThrow('Unknown check option: extra');
    expect(() => parseCli(['rust'])).toThrow('Missing Rust command.');
    expect(() => parseCli(['rust', 'fmt', 'core'])).toThrow('Unknown rust fmt option: core');
    expect(() => parseCli(['rust', 'test', 'unknown'])).toThrow("Unknown crate 'unknown'.");
    expect(() => parseCli(['iced'])).toThrow('Missing iced command.');
    expect(() => parseCli(['iced', 'run', '--unknown'])).toThrow(
      'Unknown iced run option: --unknown',
    );
    expect(() => parseCli(['monitor'])).toThrow(
      'Monitor requires --pid with a positive process id.',
    );
    expect(() =>
      parseCli(['monitor', '--pid', '1', '--out', 'target/resources/x.ndjson', '--samples', '0']),
    ).toThrow('Monitor requires --samples with a positive integer.');
  });
});
