import { beforeEach, describe, expect, test } from '@rstest/core';

import {
  FIXTURE_NETWORK_ERROR,
  createControlledInvoke,
  fixtureCallCount,
  hasExpectedServerConnectCall,
  installFixture,
  installStartupFixtures,
} from '../e2e/app/fixture-registry';

describe('native E2E fixture registry', () => {
  beforeEach(() => installStartupFixtures());

  test('returns declared raw success payloads', async () => {
    const invoke = createControlledInvoke(async () => {
      throw new Error('real IPC should not run');
    });

    await expect(invoke('server_is_connected')).resolves.toBe(false);
    await expect(invoke('server_profiles_get')).resolves.toEqual({
      activeProfileKey: null,
      profiles: [],
    });
  });

  test('rejects with the declared raw CommandError value', async () => {
    const invoke = createControlledInvoke(async () => null);

    await expect(invoke('server_connect', { credentials: {} })).rejects.toEqual(
      FIXTURE_NETWORK_ERROR,
    );
  });

  test('fails closed for undeclared commands', async () => {
    const invoke = createControlledInvoke(async () => null);

    await expect(invoke('mpv_start')).rejects.toThrow(
      'Rejected undeclared E2E IPC command: mpv_start',
    );
  });

  test('rejects real IPC outside the central allowlist', async () => {
    installFixture('server_connect', { kind: 'real' });
    const invoke = createControlledInvoke(async () => null);

    await expect(invoke('server_connect')).rejects.toThrow(
      'Rejected unsafe real E2E IPC command: server_connect',
    );
  });

  test('allows only config_default through the real IPC boundary', async () => {
    const defaults = { deviceName: 'JellyPilot' };
    const invoke = createControlledInvoke(async (command) => {
      expect(command).toBe('config_default');
      return defaults;
    });

    await expect(invoke('config_default')).resolves.toBe(defaults);
  });

  test('compares the credential fixture inside the WebView without exposing it in summaries', async () => {
    const invoke = createControlledInvoke(async () => null);

    await expect(
      invoke('server_connect', {
        credentials: {
          password: 'not-a-secret',
          provider: 'jellyfin',
          serverUrl: 'https://media.invalid',
          username: 'e2e-user',
        },
      }),
    ).rejects.toEqual(FIXTURE_NETWORK_ERROR);

    expect(fixtureCallCount('server_connect')).toBe(1);
    expect(hasExpectedServerConnectCall()).toBe(true);
  });
});
