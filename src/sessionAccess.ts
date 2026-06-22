import { Effect, Exit, Option } from 'effect';

import { commands } from './bindings';
import type { SavedSession } from './bindings';
import {
  clearSavedSession as clearSessionEffect,
  loadSavedSession as loadSessionEffect,
  saveSession as saveSessionEffect,
} from './effects/auth';
import { runTauriCommand, runTauriCommandRaw } from './effects/commands';

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
  const session = await Effect.runPromise(runTauriCommandRaw(() => commands.jellyfinGetSession()));
  Option.match(Option.fromNullishOr(session), {
    onNone: () => undefined,
    onSome: (value) => saveSession(value),
  });
}

export async function restoreSavedSession(): Promise<boolean> {
  const exit = await Effect.runPromiseExit(
    loadSessionEffect().pipe(
      Effect.flatMap((savedSession) =>
        runTauriCommand(() => commands.jellyfinRestoreSession(savedSession)),
      ),
    ),
  );
  return exit.pipe(
    Exit.match({
      onFailure: () => {
        clearSavedSession();
        return false;
      },
      onSuccess: () => true,
    }),
  );
}

export async function checkAuthWithRestore(): Promise<boolean> {
  const connected = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinIsConnected()),
  );
  if (!Exit.isSuccess(connected)) {
    return false;
  }
  if (connected.value) {
    return true;
  }
  return await restoreSavedSession();
}

export async function canAccessConsole(): Promise<boolean> {
  const connected = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinIsConnected()),
  );
  if (Exit.isSuccess(connected) && connected.value) {
    return true;
  }

  return loadSavedSession() !== null;
}
