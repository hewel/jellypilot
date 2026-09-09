import { describe, expect, test } from 'bun:test';

import { parseCrates } from './crates';

describe('crate routing', () => {
  test('rejects unknown crate aliases', () => {
    expect(() => parseCrates(['unknown'])).toThrow();
  });
});
