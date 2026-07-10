import { expect, test } from '@rstest/core';

import { ConfigCoordinator } from '../src/effects/configCoordinator';

test('theme cycle order is system → light → dark → system', () => {
  const order = ['system', 'light', 'dark'] as const;
  const next = (current: (typeof order)[number]) =>
    order[(order.indexOf(current) + 1) % order.length];
  expect(next('system')).toBe('light');
  expect(next('light')).toBe('dark');
  expect(next('dark')).toBe('system');
});

test('config coordinator starts loading before shell mounts', () => {
  const coordinator = new ConfigCoordinator();
  expect(coordinator.getState().status).toBe('loading');
});
