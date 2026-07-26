import { beforeEach, describe, expect, test } from '@rstest/core';

import {
  DEFAULT_APPEARANCE,
  DEFAULT_APPEARANCE_CANVAS,
  FIXTURE_NETWORK_ERROR,
  createControlledInvoke,
  fixtureCallCount,
  hasExpectedAppearanceReadyCall,
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

  test('allows approved read-only and readiness real IPC commands', async () => {
    const defaults = { deviceName: 'JellyPilot' };
    const invoke = createControlledInvoke(async (command) => {
      if (command === 'config_default') return defaults;
      if (command === 'appearance_ready') return null;
      if (command === 'appearance_get') {
        return { designTheme: 'braun', colorMode: 'light' };
      }
      if (command === 'plugin:window|is_visible') return false;
      if (command === 'plugin:window|theme') return 'dark';
      throw new Error(`unexpected real command: ${command}`);
    });

    installFixture('appearance_get', { kind: 'real' });

    await expect(invoke('config_default')).resolves.toBe(defaults);
    await expect(
      invoke('appearance_ready', {
        request: {
          appearance: { designTheme: 'controlRoom', colorMode: 'dark' },
          canvas: { red: 5, green: 6, blue: 10 },
        },
      }),
    ).resolves.toBeNull();
    await expect(invoke('appearance_get')).resolves.toEqual({
      designTheme: 'braun',
      colorMode: 'light',
    });
    await expect(invoke('plugin:window|is_visible', { label: 'main' })).resolves.toBe(false);
    await expect(invoke('plugin:window|theme', { label: 'main' })).resolves.toBe('dark');
  });

  test('returns the default appearance fixture on startup', async () => {
    const invoke = createControlledInvoke(async () => {
      throw new Error('real IPC should not run');
    });

    await expect(invoke('appearance_get')).resolves.toEqual({
      designTheme: 'controlRoom',
      colorMode: 'dark',
    });
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

  test('matches only the exact single appearance_ready payload', async () => {
    const invoke = createControlledInvoke(async () => null);
    const expectedAppearance = DEFAULT_APPEARANCE;
    const expectedCanvas = DEFAULT_APPEARANCE_CANVAS;

    await invoke('appearance_ready', {
      request: {
        appearance: expectedAppearance,
        canvas: expectedCanvas,
      },
    });
    expect(hasExpectedAppearanceReadyCall(expectedAppearance, expectedCanvas)).toBe(true);

    installStartupFixtures();
    await invoke('appearance_ready', {
      request: {
        appearance: { designTheme: 'braun', colorMode: 'light' },
        canvas: expectedCanvas,
      },
    });
    expect(hasExpectedAppearanceReadyCall(expectedAppearance, expectedCanvas)).toBe(false);

    installStartupFixtures();
    await invoke('appearance_ready', {
      request: {
        appearance: expectedAppearance,
        canvas: { red: 1, green: 2, blue: 3 },
      },
    });
    expect(hasExpectedAppearanceReadyCall(expectedAppearance, expectedCanvas)).toBe(false);

    installStartupFixtures();
    await invoke('appearance_ready', {
      request: {
        appearance: expectedAppearance,
        canvas: expectedCanvas,
      },
    });
    await invoke('appearance_ready', {
      request: {
        appearance: expectedAppearance,
        canvas: expectedCanvas,
      },
    });
    expect(hasExpectedAppearanceReadyCall(expectedAppearance, expectedCanvas)).toBe(false);
  });
});
