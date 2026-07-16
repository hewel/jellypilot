import { Effect } from 'effect';

import type { MediaServerProvider } from '../bindings';
import { StorageParseError } from './errors';

export const CREDENTIALS_STORAGE_KEY = 'jellypilot_saved_credentials';
export const LEGACY_CREDENTIALS_STORAGE_KEY = 'jmsr_saved_credentials';

export interface SavedCredentials {
  readonly provider: MediaServerProvider;
  readonly serverUrl: string;
  readonly username: string;
  readonly rememberMe: boolean;
}

type PersistedSavedCredentials = Omit<SavedCredentials, 'provider'> & {
  readonly provider?: MediaServerProvider;
};

function isMediaServerProvider(value: unknown): value is MediaServerProvider {
  return value === 'jellyfin' || value === 'emby';
}

function isSavedCredentials(value: unknown): value is PersistedSavedCredentials {
  if (value === null || typeof value !== 'object') {
    return false;
  }
  const obj = value as Record<string, unknown>;
  return (
    (obj.provider === undefined || isMediaServerProvider(obj.provider)) &&
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
      return yield* new StorageParseError({
        key,
        message: 'Stored credentials have an unexpected shape',
      });
    }

    return { ...parsed, provider: parsed.provider ?? 'jellyfin' };
  });
}

export const loadSavedCredentials = Effect.gen(function* () {
  const legacyCredentials = Effect.sync(() =>
    localStorage.getItem(LEGACY_CREDENTIALS_STORAGE_KEY),
  ).pipe(
    Effect.flatMap(Effect.fromNullishOr),
    Effect.flatMap((value) => parseSavedCredentials(value, LEGACY_CREDENTIALS_STORAGE_KEY)),
    Effect.tap((credentials) =>
      Effect.sync(() => {
        localStorage.setItem(CREDENTIALS_STORAGE_KEY, JSON.stringify(credentials));
        localStorage.removeItem(LEGACY_CREDENTIALS_STORAGE_KEY);
      }),
    ),
  );

  return yield* Effect.sync(() => localStorage.getItem(CREDENTIALS_STORAGE_KEY)).pipe(
    Effect.flatMap(Effect.fromNullishOr),
    Effect.matchEffect({
      onFailure: () => legacyCredentials,
      onSuccess: (value) => parseSavedCredentials(value, CREDENTIALS_STORAGE_KEY),
    }),
  );
});

export function saveCredentials(
  serverUrl: string,
  username: string,
  provider: MediaServerProvider,
): Effect.Effect<void> {
  return Effect.sync(() => {
    localStorage.setItem(
      CREDENTIALS_STORAGE_KEY,
      JSON.stringify({ provider, rememberMe: true, serverUrl, username }),
    );
    localStorage.removeItem(LEGACY_CREDENTIALS_STORAGE_KEY);
  });
}

export const clearSavedCredentials = Effect.sync(() => {
  localStorage.removeItem(CREDENTIALS_STORAGE_KEY);
  localStorage.removeItem(LEGACY_CREDENTIALS_STORAGE_KEY);
});
