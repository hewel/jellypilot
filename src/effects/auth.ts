import type { SavedSession } from '@bindings';
import { Effect } from 'effect';

import { StorageParseError } from './errors';

export const SESSION_STORAGE_KEY = 'jmsr_auth_session';

function isSavedSession(value: unknown): value is SavedSession {
  if (value === null || typeof value !== 'object') {
    return false;
  }
  const obj = value as Record<string, unknown>;
  return (
    typeof obj.serverUrl === 'string' &&
    typeof obj.accessToken === 'string' &&
    typeof obj.userId === 'string' &&
    typeof obj.userName === 'string' &&
    (obj.serverName === null || typeof obj.serverName === 'string') &&
    (obj.deviceId === null || typeof obj.deviceId === 'string')
  );
}

export function loadSavedSession(): Effect.Effect<SavedSession | null, StorageParseError> {
  return Effect.gen(function* () {
    const raw = yield* Effect.sync(() => localStorage.getItem(SESSION_STORAGE_KEY));
    if (!raw) {
      return null;
    }

    const parsed: unknown = yield* Effect.try({
      catch: () =>
        new StorageParseError({
          message: 'Could not parse saved session',
          key: SESSION_STORAGE_KEY,
        }),
      try: () => JSON.parse(raw),
    });

    if (!isSavedSession(parsed)) {
      return yield* Effect.fail(
        new StorageParseError({
          key: SESSION_STORAGE_KEY,
          message: 'Saved session has an unexpected shape',
        }),
      );
    }

    return parsed;
  });
}

export function saveSession(session: SavedSession): Effect.Effect<void> {
  return Effect.sync(() => localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session)));
}

export function clearSavedSession(): Effect.Effect<void> {
  return Effect.sync(() => localStorage.removeItem(SESSION_STORAGE_KEY));
}
