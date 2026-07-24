import { commands } from '@bindings';
import type { QuickConnectRequest, QuickConnectStatus, SavedServiceProfiles } from '@bindings';
import { Effect } from 'effect';

import { runTauriCommand } from './commands';
import { CommandError } from './errors';
import { saveCurrentServiceProfile } from './profiles';

/**
 * Shared Quick Connect polling core: starts a request, emits the code, polls
 * every 5 seconds until approved (5-minute timeout), then completes
 * authentication. All failures stay in the CommandError channel.
 */
export function runQuickConnectPolling<Result>(
  start: Effect.Effect<QuickConnectRequest, CommandError>,
  check: (secret: string) => Effect.Effect<QuickConnectStatus, CommandError>,
  authenticate: (secret: string) => Effect.Effect<Result, CommandError>,
  onCode: (code: string) => void,
): Effect.Effect<Result, CommandError> {
  return Effect.gen(function* () {
    const request = yield* start;
    yield* Effect.sync(() => onCode(request.code));

    const poll = Effect.gen(function* () {
      while (true) {
        yield* Effect.sleep(5000);
        const status = yield* check(request.secret);
        if (status === 'approved') {
          break;
        }
      }
    });

    yield* poll;

    return yield* authenticate(request.secret);
  }).pipe(
    Effect.timeout('5 minutes'),
    Effect.catchTag('TimeoutError', () =>
      Effect.fail(
        new CommandError({
          message: 'Quick Connect code expired. Request a new code to try again.',
        }),
      ),
    ),
  );
}

/**
 * Runs the Quick Connect workflow for adding a new service:
 * 1. Requests a quick connect code from the server.
 * 2. Emits the code via onCode callback.
 * 3. Polls the check endpoint every 5 seconds until approved or failed.
 * 4. Once approved, completes authentication.
 * 5. Saves the authenticated session as the active service profile.
 *
 * If 5 minutes pass without approval, it fails with a code expired error.
 */
export function runQuickConnectWorkflow(
  serverUrl: string,
  onCode: (code: string) => void,
): Effect.Effect<void, CommandError> {
  return runQuickConnectPolling(
    runTauriCommand(() => commands.jellyfinQuickConnectStart(serverUrl)),
    (secret) => runTauriCommand(() => commands.jellyfinQuickConnectCheck(serverUrl, secret)),
    (secret) =>
      Effect.gen(function* () {
        yield* runTauriCommand(() => commands.jellyfinQuickConnectAuthenticate(serverUrl, secret));
        yield* saveCurrentServiceProfile;
      }),
    onCode,
  ).pipe(Effect.asVoid);
}

/**
 * Runs the Quick Connect workflow against a saved profile's locked identity.
 * The backend resolves provider/server/account from the key; no new profile
 * is saved — authentication replaces the stored profile in place.
 */
export function runSavedProfileQuickConnectWorkflow(
  key: string,
  onCode: (code: string) => void,
): Effect.Effect<SavedServiceProfiles, CommandError> {
  return runQuickConnectPolling(
    runTauriCommand(() => commands.serverProfilesReauthenticateQuickConnectStart(key)),
    (secret) =>
      runTauriCommand(() => commands.serverProfilesReauthenticateQuickConnectCheck(key, secret)),
    (secret) =>
      runTauriCommand(() =>
        commands.serverProfilesReauthenticateQuickConnectAuthenticate(key, secret),
      ),
    onCode,
  );
}
