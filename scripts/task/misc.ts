import { Effect, Match } from 'effect';

import { command } from './commands';
import { runCommand } from './process';

export const runApi = Effect.fn('task.api')(() =>
  runCommand(command('bun', ['scripts/media-server-api.ts'])).pipe(Effect.asVoid),
);

export const runReview = Effect.fn('task.review')(
  (action: 'panda-tauri' | 'parity', args: readonly string[]) =>
    Match.value(action).pipe(
      Match.when('panda-tauri', () =>
        runCommand(
          command('bun', ['tauri', 'dev', '--config', 'src-tauri/tauri.panda-review.conf.json']),
        ).pipe(Effect.asVoid),
      ),
      Match.when('parity', () =>
        runCommand(command('bun', ['scripts/tauri-parity-review.ts', ...args])).pipe(Effect.asVoid),
      ),
      Match.exhaustive,
    ),
);
export const runPanda = Effect.fn('task.panda')(() =>
  runCommand(command('bun', ['x', 'panda', 'codegen'])).pipe(Effect.asVoid),
);
