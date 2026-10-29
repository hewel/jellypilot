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
    expect(parseCli(['iced', 'run', '--smoke', '--release', '--tv'])).toEqual({
      _tag: 'iced',
      smoke: true,
      release: true,
      embedded: false,
      tv: true,
    });
    expect(parseCli(['iced', 'hot'])).toEqual({ _tag: 'icedHot' });
  });

  test('keeps the isolated danmaku scene separate from player and backend options', () => {
    for (const option of ['--embedded', '--tv', '--file', '--release', '--smoke=false']) {
      expect(() => parseCli(['iced', 'danmaku', option])).toThrow();
    }
    expect(() => parseCli(['iced', 'danmaku', '--smoke', 'false'])).toThrow();
  });

  test('parses android commands and release flags', () => {
    expect(parseCli(['android', 'doctor'])).toEqual({ _tag: 'androidDoctor' });
    expect(parseCli(['android', 'bindings'])).toEqual({ _tag: 'androidBindings' });
    expect(parseCli(['android', 'mpv'])).toEqual({ _tag: 'androidMpv' });
    expect(parseCli(['android', 'check'])).toEqual({ _tag: 'androidCheck' });
    expect(parseCli(['android', 'rust'])).toEqual({ _tag: 'androidRust', release: false });
    expect(parseCli(['android', 'rust', '--release'])).toEqual({
      _tag: 'androidRust',
      release: true,
    });
    expect(parseCli(['android', 'build', '--release'])).toEqual({
      _tag: 'androidBuild',
      release: true,
    });
  });

  test('rejects unknown android actions and stray options', () => {
    expect(() => parseCli(['android'])).toThrow('Missing Android command.');
    expect(() => parseCli(['android', 'emulator'])).toThrow('Unknown Android command: emulator');
    expect(() => parseCli(['android', 'doctor', '--release'])).toThrow(
      'Unknown android doctor option: --release',
    );
    expect(() => parseCli(['android', 'mpv', 'arm64-v8a'])).toThrow(
      'Unknown android mpv option: arm64-v8a',
    );
    expect(() => parseCli(['android', 'rust', '--target'])).toThrow(
      'Unknown android rust option: --target',
    );
  });

  test('parses private Linux runtime builds with an optional mpv checkout', () => {
    expect(parseCli(['mpv', 'linux-runtime'])).toEqual({
      _tag: 'mpvLinuxRuntime',
      source: null,
    });
    expect(parseCli(['mpv', 'linux-runtime', '--source', 'checkout'])).toEqual({
      _tag: 'mpvLinuxRuntime',
      source: 'checkout',
    });
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
      ['linux-runtime', '--source'],
      ['linux-runtime', '--source', 'checkout', '--source', 'other'],
      ['linux-runtime', 'checkout'],
      ['linux-runtime', '--release'],
      ['run'],
    ]) {
      expect(() => parseCli(['mpv', ...args])).toThrow();
    }
  });

  test('requires an explicit Windows runtime directory without accepting extra package options', () => {
    expect(
      parseCli(['package', 'windows', '--runtime-dir', String.raw`C:\msys64\ucrt64\bin`]),
    ).toEqual({
      _tag: 'packageWindows',
      runtimeDirectory: 'C:\\msys64\\ucrt64\\bin',
    });
    for (const args of [
      [],
      ['windows'],
      ['windows', '--runtime-dir'],
      ['windows', '--runtime-dir', ' '],
      ['windows', '--runtime-dir', '--release'],
      ['windows', '--runtime-dir', 'bin', '--runtime-dir', 'other'],
      ['linux', '--runtime-dir', 'bin'],
    ]) {
      expect(() => parseCli(['package', ...args])).toThrow('Expected package windows');
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

  test('bounds sustained GPU duration without changing the default short regression', () => {
    const gpu = ['iced', 'regress', 'gpu', '--file', 'clip.mp4'];
    expect(parseCli(gpu)).toMatchObject({ durationSeconds: null });
    for (const [input, durationSeconds] of [
      ['1', 1],
      ['3600', 3600],
      ['00030', 30],
    ] as const) {
      expect(parseCli([...gpu, '--duration-secs', input])).toMatchObject({ durationSeconds });
    }
  });

  test('rejects non-decimal, unbounded, missing and duplicate sustained durations', () => {
    const gpu = ['iced', 'regress', 'gpu', '--file', 'clip.mp4'];
    for (const value of [
      '',
      ' ',
      '0',
      '000',
      '3601',
      '9007199254740993',
      '9'.repeat(400),
      '-1',
      '+1',
      '1.0',
      '1.5',
      '1e2',
      '0x10',
      'Infinity',
      'NaN',
      ' 30',
      '30 ',
      '30\n',
      '3\n0',
      '３０',
      '٣٠',
      '--out',
    ]) {
      expect(() => parseCli([...gpu, '--duration-secs', value])).toThrow();
    }
    expect(() => parseCli([...gpu, '--duration-secs'])).toThrow();
    for (const repeated of ['30', '60']) {
      expect(() =>
        parseCli([...gpu, '--duration-secs', '30', '--duration-secs', repeated]),
      ).toThrow();
    }
  });

  test('does not enable sustained mode for joint or non-GPU regression selections', () => {
    for (const scenario of ['tray', 'external', 'all']) {
      const media = scenario === 'all' ? ['--file', 'clip.mp4'] : [];
      expect(() =>
        parseCli(['iced', 'regress', scenario, ...media, '--duration-secs', '30']),
      ).toThrow();
    }
    expect(() => parseCli(['iced', 'run', '--duration-secs', '30'])).toThrow();
  });

  test('selects timeline v3 explicitly without changing the legacy regression baseline', () => {
    expect(parseCli(['iced', 'regress', 'tray'])).toMatchObject({ hostAbi: 'legacy' });
    expect(
      parseCli(['iced', 'regress', 'all', '--file', 'clip.mp4', '--host-abi', '3']),
    ).toMatchObject({ scenario: 'all', hostAbi: '3' });
    expect(parseCli(['iced', 'regress', 'external', '--host-abi', 'legacy'])).toMatchObject({
      scenario: 'external',
      hostAbi: 'legacy',
    });
  });

  test('rejects unsupported, missing and conflicting host ABI selectors', () => {
    for (const args of [
      ['--host-abi'],
      ['--host-abi', ' '],
      ['--host-abi', '--out', 'target/probe'],
      ['--host-abi', '1'],
      ['--host-abi', '2'],
      ['--host-abi', 'v3'],
      ['--host-abi', '3', '--host-abi', 'legacy'],
    ]) {
      expect(() => parseCli(['iced', 'regress', 'tray', ...args])).toThrow();
    }
    expect(() => parseCli(['iced', 'run', '--host-abi', '3'])).toThrow();
  });

  test('does not accept embedded selection on unrelated iced commands or as a valued option', () => {
    expect(() => parseCli(['iced', 'hot', '--embedded'])).toThrow();
    expect(() => parseCli(['iced', 'run', '--embedded', 'false'])).toThrow();
    expect(() => parseCli(['iced', 'run', '--embedded=false'])).toThrow();
    expect(() => parseCli(['iced', 'run', '--tv=false'])).toThrow();
    expect(() => parseCli(['iced', 'hot', '--tv'])).toThrow();
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
