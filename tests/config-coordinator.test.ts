import { expect, rstest, test } from '@rstest/core';
import { waitFor } from '@testing-library/dom';

import { commands } from '../src/bindings';
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

test('failed queued save rolls back to the latest persisted configuration', async () => {
  rstest.spyOn(commands, 'configGet').mockResolvedValue({ themePreference: 'system' });
  const firstSave = Promise.withResolvers<Awaited<ReturnType<typeof commands.configSet>>>();
  const secondSave = Promise.withResolvers<Awaited<ReturnType<typeof commands.configSet>>>();
  const configSet = rstest
    .spyOn(commands, 'configSet')
    .mockImplementationOnce(() => firstSave.promise)
    .mockImplementationOnce(() => secondSave.promise);
  const coordinator = new ConfigCoordinator();
  await coordinator.bootstrap();

  coordinator.setThemePreference('light');
  coordinator.setThemePreference('dark');
  firstSave.resolve({ data: null, status: 'ok' });
  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(2));

  secondSave.resolve({
    error: { code: 'internal', message: 'Config write failed' },
    status: 'error',
  });
  await waitFor(() => expect(coordinator.getState().status).toBe('error'));

  expect(coordinator.getState().confirmed?.themePreference).toBe('light');
  expect(coordinator.getState().desired?.themePreference).toBe('light');
});
