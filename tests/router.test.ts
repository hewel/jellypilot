import { afterEach, expect, rstest, test } from '@rstest/core';

import { commands } from '../src/bindings';
import type { SavedSession } from '../src/bindings';
import {
  createJellyPilotRouter,
  redirectLegacyConsoleRoute,
  redirectLoggedInUsersToLibrary,
  redirectRootRoute,
  requireAuthenticatedShell,
} from '../src/router';
import { saveSession } from '../src/sessionAccess';

const sampleSession: SavedSession = {
  accessToken: 'token-1',
  deviceId: 'device-1',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
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
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(true);

  await expectRedirect(redirectLoggedInUsersToLibrary, '/library');
});

test('root guard restores a Saved Session into Library', async () => {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'jellyfinRestoreSession').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  saveSession(sampleSession);

  await expectRedirect(redirectRootRoute, '/library');
});

test('legacy console redirects authenticated users to Library', async () => {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(true);

  await expectRedirect(redirectLegacyConsoleRoute, '/library');
});

test('shell guard redirects unauthenticated users to Login', async () => {
  rstest.spyOn(commands, 'jellyfinIsConnected').mockResolvedValue(false);

  await expectRedirect(requireAuthenticatedShell, '/login');
});

test('removed Settings and Diagnostics routes are absent from the router', () => {
  const router = createJellyPilotRouter();

  expect(router.routesById['/_authenticated/settings']).toBeUndefined();
  expect(router.routesById['/_authenticated/diagnostics']).toBeUndefined();
});
