import { describe, expect, test } from 'bun:test';

import { icedLocalVideoCommand } from './iced';
import { parseCli } from './parse';
import { formatCommand } from './process';

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

  test('preserves a signed URL through private child environment without cargo command exposure', () => {
    const url = 'https://media.example/中文 clips/movie.m3u8?token=a%2Bb&name="clip";$(id)';
    const task = parseCli(['iced', 'local-video', 'run', '--url', url, '--smoke']);
    if (task._tag !== 'icedLocalVideo' || task.action !== 'run') {
      throw new Error('Expected local video run task.');
    }
    expect(task.file).toBeNull();
    const request = icedLocalVideoCommand(task);
    expect(request.env?.JELLYPILOT_VIDEO_URL).toBe(url);
    expect(request.args.slice(request.args.indexOf('--') + 1)).toEqual([
      '--smoke-test',
      '--url-env',
    ]);
    expect(formatCommand(request)).not.toContain(url);
    expect(formatCommand(request)).not.toContain('token=');
  });

  test('keeps an explicitly selected URL-looking filename local', () => {
    const file = 'https://media.example/movie.mp4';
    const task = parseCli(['iced', 'local-video', 'run', '--file', file]);
    if (task._tag !== 'icedLocalVideo' || task.action !== 'run') {
      throw new Error('Expected local video run task.');
    }
    expect(task.url).toBeNull();
    const { args } = icedLocalVideoCommand(task);
    expect(args.slice(args.indexOf('--') + 1)).toEqual(['--file', file]);
  });

  test('rejects ambiguous or incomplete network source options before launching cargo', () => {
    const url = 'https://media.example/movie.m3u8?token=private';
    for (const args of [
      ['run', '--url'],
      ['run', '--url', ''],
      ['run', '--url', '   '],
      ['run', '--url', '--smoke'],
      ['run', '--url', url, '--url', url],
      ['run', '--url', url, '--file', 'movie.mp4'],
      ['run', '--file', 'movie.mp4', '--url', url],
      ['run', '--url-env', '--file', 'movie.mp4'],
      ['run', '--url', url, '--url-env'],
      ['run', '--url-env', '--url-env'],
      ['check', '--url', url],
      ['test', '--url', url],
      ['clippy', '--url', url],
      ['fmt', '--url', url],
      ['run', '--header', 'Authorization: private'],
    ]) {
      expect(() => parseCli(['iced', 'local-video', ...args])).toThrow();
    }
  });

  test('never includes rejected source or option credentials in parser errors', () => {
    const secret = 'credential-must-not-appear';
    for (const args of [
      ['run', `--url=https://media.example/?token=${secret}`],
      ['run', `https://media.example/?token=${secret}`],
      ['check', `--url=${secret}`],
      ['test', `--url=${secret}`],
      ['fmt', `--url=${secret}`],
      [`https://media.example/?token=${secret}`],
    ]) {
      let failure: unknown;
      try {
        parseCli(['iced', 'local-video', ...args]);
      } catch (error) {
        failure = error;
      }
      expect(failure).toBeInstanceOf(Error);
      expect(String(failure)).not.toContain(secret);
    }
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
