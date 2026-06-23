import { commands } from '@bindings';
import { Effect } from 'effect';

import { runTauriCommand } from './commands';
import { CommandError } from './errors';
import { saveCurrentServiceProfile } from './profiles';

/**
 * Runs the Quick Connect workflow:
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
  return Effect.gen(function* () {
    const request = yield* runTauriCommand(() => commands.jellyfinQuickConnectStart(serverUrl));
    yield* Effect.sync(() => onCode(request.code));

    const poll = Effect.gen(function* () {
      while (true) {
        yield* Effect.sleep(5000);
        const status = yield* runTauriCommand(() =>
          commands.jellyfinQuickConnectCheck(serverUrl, request.secret),
        );
        if (status === 'approved') {
          break;
        }
      }
    });

    yield* poll;

    yield* runTauriCommand(() =>
      commands.jellyfinQuickConnectAuthenticate(serverUrl, request.secret),
    );

    yield* saveCurrentServiceProfile();
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
