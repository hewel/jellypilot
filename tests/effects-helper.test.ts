import { expect, test } from '@rstest/core';
import { Exit } from 'effect';

import { defaultTo } from '../src/effects/helper';

test('defaultTo returns the successful Exit value', () => {
  const value = defaultTo('fallback')(Exit.succeed('loaded'));

  expect(value).toBe('loaded');
});

test('defaultTo returns the fallback value for a failed Exit', () => {
  const value = defaultTo('fallback')(Exit.fail('failed'));

  expect(value).toBe('fallback');
});
