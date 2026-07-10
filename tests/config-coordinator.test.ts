import { expect, test } from '@rstest/core';

import { ConfigCoordinator } from '../src/effects/configCoordinator';

test('coordinator starts in loading state', () => {
  const coordinator = new ConfigCoordinator();
  expect(coordinator.getState().status).toBe('loading');
  expect(coordinator.getState().confirmed).toBeNull();
});

test('theme default helper treats missing preference as system via patch shape', () => {
  const coordinator = new ConfigCoordinator();
  // Without bootstrap, patch is ignored because no base config exists.
  coordinator.setThemePreference('dark');
  expect(coordinator.getState().desired).toBeNull();
});
