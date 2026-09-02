import { Effect } from 'effect';

import { command } from './commands';
import { runCommand } from './process';

export const runPromo = Effect.fn('task.promo')(() =>
  runCommand(command('bun', ['scripts/render-promo.ts'])).pipe(Effect.asVoid),
);
