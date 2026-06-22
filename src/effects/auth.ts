import type { SavedSession } from '@bindings';
import { Effect } from 'effect';

import { StorageParseError } from './errors';

export const SESSION_STORAGE_KEY = 'jellypilot_auth_session';
export const LEGACY_SESSION_STORAGE_KEY = 'jmsr_auth_session';

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

function parseSavedSession(
  raw: string,
  key: string,
): Effect.Effect<SavedSession, StorageParseError> {
  return Effect.gen(function* () {
    const parsed: unknown = yield* Effect.try({
      catch: () =>
        new StorageParseError({
          message: 'Could not parse saved session',
          key,
        }),
      try: () => JSON.parse(raw),
    });

    if (!isSavedSession(parsed)) {
      return yield* Effect.fail(
        new StorageParseError({
          key,
          message: 'Saved session has an unexpected shape',
        }),
      );
    }

    return parsed;
  });
}

function normalizeLegacySession(session: SavedSession): SavedSession {
  return session.deviceId?.startsWith('jmsr-') ? { ...session, deviceId: null } : session;
}

export function loadSavedSession(): Effect.Effect<SavedSession | null, StorageParseError> {
  return Effect.gen(function* () {
    const raw = yield* Effect.sync(() => localStorage.getItem(SESSION_STORAGE_KEY));
    if (raw !== null) {
      return yield* parseSavedSession(raw, SESSION_STORAGE_KEY);
    }

    const legacyRaw = yield* Effect.sync(() => localStorage.getItem(LEGACY_SESSION_STORAGE_KEY));
    if (legacyRaw === null) {
      return null;
    }

    const session = normalizeLegacySession(
      yield* parseSavedSession(legacyRaw, LEGACY_SESSION_STORAGE_KEY),
    );
    yield* Effect.sync(() => {
      localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
      localStorage.removeItem(LEGACY_SESSION_STORAGE_KEY);
    });

    return session;
  });
}

export function saveSession(session: SavedSession): Effect.Effect<void> {
  return Effect.sync(() => {
    localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
    localStorage.removeItem(LEGACY_SESSION_STORAGE_KEY);
  });
}

export function clearSavedSession(): Effect.Effect<void> {
  return Effect.sync(() => {
    localStorage.removeItem(SESSION_STORAGE_KEY);
    localStorage.removeItem(LEGACY_SESSION_STORAGE_KEY);
  });
}
