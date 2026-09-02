import { Effect } from 'effect';

import { command } from './commands';
import { runCommand } from './process';

export const runApi = Effect.fn('task.api')(() =>
  runCommand(command('bun', ['scripts/media-server-api.ts'])).pipe(Effect.asVoid),
);
