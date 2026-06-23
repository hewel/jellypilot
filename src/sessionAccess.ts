import { Effect, Exit, Option } from 'effect';

import { commands } from './bindings';
import type { SavedSession } from './bindings';
import {
  clearSavedSession as clearSessionEffect,
  loadSavedSession as loadSessionEffect,
  saveSession as saveSessionEffect,
} from './effects/auth';
import { runTauriCommandRaw } from './effects/commands';
import {
  activateSavedServiceProfile,
  fetchSavedServiceProfiles,
  importLegacySavedSession,
  saveCurrentServiceProfile,
} from './effects/profiles';

export function loadSavedSession(): SavedSession | null {
  return loadSessionEffect().pipe(
    Effect.runSyncExit,
    Exit.match({
      onFailure: () => null,
      onSuccess: (v) => v,
    }),
  );
}

export function saveSession(session: SavedSession): void {
  Effect.runSync(saveSessionEffect(session));
}

export function clearSavedSession(): void {
  Effect.runSync(clearSessionEffect());
}

export async function saveCurrentSession(): Promise<void> {
  await Effect.runPromise(saveCurrentServiceProfile());
  clearSavedSession();
}

async function migrateLegacySavedSession(): Promise<void> {
  const session = loadSavedSession();
  if (!session) {
    return;
  }

  const exit = await Effect.runPromiseExit(importLegacySavedSession(session));
  if (Exit.isSuccess(exit)) {
    clearSavedSession();
  }
}

export async function restoreSavedSession(): Promise<boolean> {
  await migrateLegacySavedSession();
  const profiles = await Effect.runPromiseExit(fetchSavedServiceProfiles());
  if (!Exit.isSuccess(profiles)) {
    return false;
  }

  return await Option.match(Option.fromNullishOr(profiles.value.activeProfileKey), {
    onNone: async () => false,
    onSome: async (key) =>
      Exit.isSuccess(await Effect.runPromiseExit(activateSavedServiceProfile(key))),
  });
}

export async function checkAuthWithRestore(): Promise<boolean> {
  const connected = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.serverIsConnected()),
  );
  if (!Exit.isSuccess(connected)) {
    return false;
  }
  if (connected.value) {
    return true;
  }

  await migrateLegacySavedSession();
  const profiles = await Effect.runPromiseExit(fetchSavedServiceProfiles());
  if (!Exit.isSuccess(profiles)) {
    return false;
  }

  const activeProfileKey = Option.fromNullishOr(profiles.value.activeProfileKey);
  if (Option.isNone(activeProfileKey)) {
    return profiles.value.profiles.length > 0;
  }

  const restored = await Effect.runPromiseExit(activateSavedServiceProfile(activeProfileKey.value));
  return Exit.isSuccess(restored) || profiles.value.profiles.length > 0;
}

export async function canAccessConsole(): Promise<boolean> {
  const connected = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.serverIsConnected()),
  );
  if (Exit.isSuccess(connected) && connected.value) {
    return true;
  }

  await migrateLegacySavedSession();
  const profiles = await Effect.runPromiseExit(fetchSavedServiceProfiles());
  return Exit.isSuccess(profiles) && profiles.value.profiles.length > 0;
}
