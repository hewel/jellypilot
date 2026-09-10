import { describe, expect, test } from 'bun:test';

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
      embedded: false,
    });
    expect(parseCli(['iced', 'hot'])).toEqual({ _tag: 'icedHot' });
  });

  test('rejects missing, duplicate, option-like and extra mpv source arguments', () => {
    for (const args of [
      ['build', '--source'],
      ['build', '--source', ' '],
      ['build', '--source', '--release'],
      ['build', '--source', 'checkout', '--source', 'other'],
      ['build', 'checkout'],
      ['build', '--source', 'checkout', '--push'],
      ['build', '--release'],
      ['run'],
    ]) {
      expect(() => parseCli(['mpv', ...args])).toThrow();
    }
  });

  test('rejects GPU regression without media before dispatching preparation or startup', () => {
    for (const args of [
      ['gpu'],
      ['all', '--out', 'target/probe'],
      ['gpu', '--file'],
      ['all', '--file', ' '],
      ['gpu', '--file', '--out'],
      ['gpu', '--file', 'clip.mp4', '--file', 'other.mp4'],
      ['tray', '--file', 'clip.mp4'],
      ['tray', '--hwdec', 'vaapi'],
      ['gpu', '--file', 'clip.mp4', '--hwdec', 'auto'],
      ['gpu', '--file', 'clip.mp4', '--hwdec', 'vaapi', '--hwdec', 'no'],
    ]) {
      expect(() => parseCli(['iced', 'regress', ...args])).toThrow();
    }
  });

  test('preserves an explicit decoder override for the GPU extra-args probe', () => {
    expect(
      parseCli(['iced', 'regress', 'gpu', '--file', 'clip.mp4', '--hwdec', 'vaapi-copy']),
    ).toMatchObject({ _tag: 'icedRegress', file: 'clip.mp4', hwdec: 'vaapi-copy' });
  });

  test('does not accept embedded selection on unrelated iced commands or as a valued option', () => {
    expect(() => parseCli(['iced', 'hot', '--embedded'])).toThrow();
    expect(() => parseCli(['iced', 'run', '--embedded', 'false'])).toThrow();
    expect(() => parseCli(['iced', 'run', '--embedded=false'])).toThrow();
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
