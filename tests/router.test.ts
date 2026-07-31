// @rstest-environment jsdom
import { afterEach, expect, rstest, test } from '@rstest/core';
import { createMemoryHistory } from '@tanstack/solid-router';

import { commands } from '../src/bindings';
import type { SavedServiceProfiles } from '../src/bindings';
import {
  createJellyPilotRouter,
  redirectLoggedInUsersToLibrary,
  redirectRootRoute,
  requireAuthenticatedShell,
} from '../src/router';

const sampleProfiles: SavedServiceProfiles = {
  activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
  profiles: [
    {
      active: true,
      key: 'jellyfin|https://jellyfin.example.com|Ada',
      lastRestoreError: null,
      reauthRequired: false,
      provider: 'jellyfin',
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
  ],
};

async function expectRedirect(action: () => Promise<void>, expectedRoute: string) {
  try {
    await action();
    throw new Error('Expected redirect');
  } catch (error) {
    expect(JSON.stringify(error)).toContain(`"to":"${expectedRoute}"`);
  }
}

afterEach(() => {
  rstest.restoreAllMocks();
  localStorage.clear();
});

test('login guard redirects authenticated users to Library', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(true);

  await expectRedirect(redirectLoggedInUsersToLibrary, '/library');
});

test('root guard restores the active saved service profile into Library', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const activate = rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });

  await expectRedirect(redirectRootRoute, '/library');
  expect(activate).toHaveBeenCalledWith(sampleProfiles.activeProfileKey);
});

test('shell guard redirects unauthenticated users to Login', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: { activeProfileKey: null, profiles: [] },
    status: 'ok',
  });

  await expectRedirect(requireAuthenticatedShell, '/login');
});

test('shell guard restores the active saved service profile on deep links', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const activate = rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });

  await requireAuthenticatedShell();

  expect(activate).toHaveBeenCalledWith(sampleProfiles.activeProfileKey);
});

test('shell guard admits retained profiles when active profile restore fails', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    error: { code: 'authFailed', message: 'expired' },
    status: 'error',
  });

  await requireAuthenticatedShell();
});

test('shell guard makes one restore decision and does not double-activate', async () => {
  const connected = rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const activate = rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });

  await Promise.all([requireAuthenticatedShell(), requireAuthenticatedShell()]);
  expect(activate).toHaveBeenCalledTimes(1);

  connected.mockResolvedValue(true);
  await requireAuthenticatedShell();
  expect(activate).toHaveBeenCalledTimes(1);
});

test('browse route redirects unknown collection types to Library', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(true);
  const router = createJellyPilotRouter(
    createMemoryHistory({ initialEntries: ['/library/books/abc'] }),
  );

  await router.load();

  expect(router.state.location.pathname).toBe('/library');
});

test('removed Settings, Diagnostics, and Console routes are absent from the router', () => {
  const router = createJellyPilotRouter();

  expect(router.routesById['/_authenticated/settings']).toBeUndefined();
  expect(router.routesById['/_authenticated/diagnostics']).toBeUndefined();
  expect(router.routesById['/console']).toBeUndefined();
});
