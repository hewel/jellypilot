import { Effect } from 'effect';

import { StorageParseError } from './errors';

export const CREDENTIALS_STORAGE_KEY = 'jellypilot_saved_credentials';
export const LEGACY_CREDENTIALS_STORAGE_KEY = 'jmsr_saved_credentials';

export interface SavedCredentials {
  readonly serverUrl: string;
  readonly username: string;
  readonly rememberMe: boolean;
}

function isSavedCredentials(value: unknown): value is SavedCredentials {
  if (value === null || typeof value !== 'object') {
    return false;
  }
  const obj = value as Record<string, unknown>;
  return (
    typeof obj.serverUrl === 'string' &&
    typeof obj.username === 'string' &&
    typeof obj.rememberMe === 'boolean'
  );
}

function parseSavedCredentials(
  raw: string,
  key: string,
): Effect.Effect<SavedCredentials, StorageParseError> {
  return Effect.gen(function* () {
    const parsed: unknown = yield* Effect.try({
      catch: () =>
        new StorageParseError({
          message: 'Could not parse stored credentials',
          key,
        }),
      try: () => JSON.parse(raw),
    });

    if (!isSavedCredentials(parsed)) {
      return yield* Effect.fail(
        new StorageParseError({
          key,
          message: 'Stored credentials have an unexpected shape',
        }),
      );
    }

    return parsed;
  });
}

export function loadSavedCredentials(): Effect.Effect<SavedCredentials | null, StorageParseError> {
  return Effect.gen(function* () {
    const raw = yield* Effect.sync(() => localStorage.getItem(CREDENTIALS_STORAGE_KEY));
    if (raw !== null) {
      return yield* parseSavedCredentials(raw, CREDENTIALS_STORAGE_KEY);
    }

    const legacyRaw = yield* Effect.sync(() =>
      localStorage.getItem(LEGACY_CREDENTIALS_STORAGE_KEY),
    );
    if (legacyRaw === null) {
      return null;
    }

    const credentials = yield* parseSavedCredentials(legacyRaw, LEGACY_CREDENTIALS_STORAGE_KEY);
    yield* Effect.sync(() => {
      localStorage.setItem(CREDENTIALS_STORAGE_KEY, JSON.stringify(credentials));
      localStorage.removeItem(LEGACY_CREDENTIALS_STORAGE_KEY);
    });

    return credentials;
  });
}

export function saveCredentials(serverUrl: string, username: string): Effect.Effect<void> {
  return Effect.sync(() => {
    localStorage.setItem(
      CREDENTIALS_STORAGE_KEY,
      JSON.stringify({ rememberMe: true, serverUrl, username }),
    );
    localStorage.removeItem(LEGACY_CREDENTIALS_STORAGE_KEY);
  });
}

export function clearSavedCredentials(): Effect.Effect<void> {
  return Effect.sync(() => {
    localStorage.removeItem(CREDENTIALS_STORAGE_KEY);
    localStorage.removeItem(LEGACY_CREDENTIALS_STORAGE_KEY);
  });
}
