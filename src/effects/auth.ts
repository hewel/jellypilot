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

export function loadSavedSession() {
  const legacySession = Effect.sync(() => localStorage.getItem(LEGACY_SESSION_STORAGE_KEY)).pipe(
    Effect.flatMap(Effect.fromNullishOr),
    Effect.flatMap((value) => parseSavedSession(value, LEGACY_SESSION_STORAGE_KEY)),
    Effect.map(normalizeLegacySession),
    Effect.tap((session) =>
      Effect.sync(() => {
        localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
        localStorage.removeItem(LEGACY_SESSION_STORAGE_KEY);
      }),
    ),
  );
  return Effect.sync(() => localStorage.getItem(SESSION_STORAGE_KEY)).pipe(
    Effect.flatMap(Effect.fromNullishOr),
    Effect.matchEffect({
      onFailure: () => legacySession,
      onSuccess: (value) => parseSavedSession(value, SESSION_STORAGE_KEY),
    }),
  );
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
