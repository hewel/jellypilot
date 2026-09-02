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
    });
    expect(parseCli(['iced', 'hot'])).toEqual({ _tag: 'icedHot' });
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
  });
});
