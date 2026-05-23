import { Effect, Exit } from 'effect';
import { commands, type SavedSession } from './bindings';
import {
  clearSavedSession as clearSessionEffect,
  loadSavedSession as loadSessionEffect,
  saveSession as saveSessionEffect,
} from './effects/auth';
import { runTauriCommand, runTauriCommandRaw } from './effects/commands';

export function loadSavedSession(): SavedSession | null {
  const exit = Effect.runSyncExit(loadSessionEffect());
  if (Exit.isSuccess(exit)) return exit.value;
  return null;
}

export function saveSession(session: SavedSession): void {
  Effect.runSync(saveSessionEffect(session));
}

export function clearSavedSession(): void {
  Effect.runSync(clearSessionEffect());
}

export async function saveCurrentSession(): Promise<void> {
  const session = await commands.jellyfinGetSession();
  if (session) saveSession(session);
}

export async function restoreSavedSession(): Promise<boolean> {
  const savedSession = loadSavedSession();
  if (!savedSession) return false;

  const exit = await Effect.runPromiseExit(
    runTauriCommand(() => commands.jellyfinRestoreSession(savedSession)),
  );
  if (Exit.isSuccess(exit)) return true;

  clearSavedSession();
  return false;
}

export async function checkAuthWithRestore(): Promise<boolean> {
  const connected = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinIsConnected()),
  );
  if (!Exit.isSuccess(connected)) return false;
  if (connected.value) return true;
  return await restoreSavedSession();
}

export async function canAccessConsole(): Promise<boolean> {
  const connected = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinIsConnected()),
  );
  return (
    (Exit.isSuccess(connected) && connected.value) ||
    loadSavedSession() !== null
  );
}
