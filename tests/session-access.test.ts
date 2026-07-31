// @rstest-environment jsdom
import { afterEach, expect, rstest, test } from '@rstest/core';

import { commands } from '../src/bindings';
import type { SavedServiceProfiles, SavedSession } from '../src/bindings';
import { LEGACY_SESSION_STORAGE_KEY, SESSION_STORAGE_KEY } from '../src/effects/auth';
import {
  canAccessConsole,
  clearSavedSession,
  loadSavedSession,
  resolveAuthenticatedEntry,
  restoreSavedSession,
  saveSession,
} from '../src/sessionAccess';

const sampleSession: SavedSession = {
  accessToken: 'token-1',
  deviceId: 'device-1',
  provider: 'jellyfin',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};
const sampleProfileKey = 'jellyfin|https://jellyfin.example.com|Ada';
const sampleProfiles: SavedServiceProfiles = {
  activeProfileKey: sampleProfileKey,
  profiles: [
    {
      active: true,
      key: sampleProfileKey,
      lastRestoreError: null,
      reauthRequired: false,
      provider: 'jellyfin',
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
  ],
};

afterEach(() => {
  rstest.restoreAllMocks();
  localStorage.clear();
});

test('canAccessConsole allows connected users without a Saved Session', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(true);

  await expect(canAccessConsole()).resolves.toBe(true);
});

test('canAccessConsole allows disconnected users with a saved service profile', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });

  await expect(canAccessConsole()).resolves.toBe(true);
});

test('canAccessConsole denies disconnected users without a saved service profile', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: { activeProfileKey: null, profiles: [] },
    status: 'ok',
  });

  await expect(canAccessConsole()).resolves.toBe(false);
});

test('canAccessConsole falls back to saved profiles when connected check throws', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockRejectedValue(new Error('ipc unavailable'));
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });

  await expect(canAccessConsole()).resolves.toBe(true);
});

test('restoreSavedSession restores the live connection from the active saved service profile', async () => {
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const activate = rstest
    .spyOn(commands, 'serverProfilesActivate')
    .mockResolvedValue({ data: sampleProfiles, status: 'ok' });

  await expect(restoreSavedSession()).resolves.toBe(true);
  expect(activate).toHaveBeenCalledWith(sampleProfileKey);
});

test('restoreSavedSession returns false after active profile restore failure', async () => {
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    error: { code: 'authFailed', message: 'expired' },
    status: 'error',
  });

  await expect(restoreSavedSession()).resolves.toBe(false);
});

test('restoreSavedSession returns false after active profile restore command throws', async () => {
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesActivate').mockRejectedValue(new Error('ipc unavailable'));

  await expect(restoreSavedSession()).resolves.toBe(false);
});

test('resolveAuthenticatedEntry attempts active profile restore before denying root route access', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const activate = rstest
    .spyOn(commands, 'serverProfilesActivate')
    .mockResolvedValue({ data: sampleProfiles, status: 'ok' });

  await expect(resolveAuthenticatedEntry()).resolves.toBe(true);
  expect(activate).toHaveBeenCalledWith(sampleProfileKey);
});

test('resolveAuthenticatedEntry denies access when command checks throw', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockRejectedValue(new Error('ipc unavailable'));
  rstest.spyOn(commands, 'serverProfilesGet').mockRejectedValue(new Error('profiles unavailable'));

  await expect(resolveAuthenticatedEntry()).resolves.toBe(false);
});

test('resolveAuthenticatedEntry denies access without activating when no profiles are saved', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: { activeProfileKey: null, profiles: [] },
    status: 'ok',
  });
  const activate = rstest.spyOn(commands, 'serverProfilesActivate');

  await expect(resolveAuthenticatedEntry()).resolves.toBe(false);
  expect(activate).not.toHaveBeenCalled();
});

test('resolveAuthenticatedEntry allows shell access when active profile restore fails but profiles remain', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    error: { code: 'authFailed', message: 'expired' },
    status: 'error',
  });

  await expect(resolveAuthenticatedEntry()).resolves.toBe(true);
});

test('resolveAuthenticatedEntry shares one restore decision across concurrent entries', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const activate = rstest
    .spyOn(commands, 'serverProfilesActivate')
    .mockResolvedValue({ data: sampleProfiles, status: 'ok' });

  const [first, second] = await Promise.all([
    resolveAuthenticatedEntry(),
    resolveAuthenticatedEntry(),
  ]);

  expect(first).toBe(true);
  expect(second).toBe(true);
  expect(activate).toHaveBeenCalledTimes(1);
});

test('clearSavedSession removes Saved Session state synchronously', () => {
  saveSession(sampleSession);

  clearSavedSession();

  expect(loadSavedSession()).toBeNull();
});

test('migrates legacy Saved Session storage and clears the old key', () => {
  localStorage.setItem(
    LEGACY_SESSION_STORAGE_KEY,
    JSON.stringify({ ...sampleSession, deviceId: 'jmsr-saved-device' }),
  );

  expect(loadSavedSession()).toEqual({ ...sampleSession, deviceId: null });
  expect(localStorage.getItem(LEGACY_SESSION_STORAGE_KEY)).toBeNull();
  expect(localStorage.getItem(SESSION_STORAGE_KEY)).not.toBeNull();
});

test('canAccessConsole imports legacy Saved Session into saved profiles', async () => {
  rstest.spyOn(commands, 'serverIsConnected').mockResolvedValue(false);
  const importLegacy = rstest.spyOn(commands, 'serverProfilesImportLegacy').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(sampleSession));

  await expect(canAccessConsole()).resolves.toBe(true);
  expect(importLegacy).toHaveBeenCalledWith(sampleSession);
  expect(localStorage.getItem(SESSION_STORAGE_KEY)).toBeNull();
});

test('loads Saved Sessions without provider as Jellyfin sessions', () => {
  const { provider: _provider, ...legacySession } = sampleSession;

  localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(legacySession));

  expect(loadSavedSession()).toEqual({ ...sampleSession, provider: 'jellyfin' });
});

test('clearSavedSession clears Saved Session legacy storage', () => {
  localStorage.setItem(LEGACY_SESSION_STORAGE_KEY, JSON.stringify(sampleSession));

  clearSavedSession();

  expect(localStorage.getItem(LEGACY_SESSION_STORAGE_KEY)).toBeNull();
});
