import { Effect } from 'effect';

import { command } from './commands';
import { runCommand } from './process';

export const runIced = Effect.fn('task.iced')((smoke: boolean) =>
  runCommand(
    command('cargo', [
      'run',
      '--manifest-path',
      'Cargo.toml',
      '--package',
      'jellypilot-iced',
      ...(smoke ? ['--', '--smoke-test'] : []),
    ]),
  ).pipe(Effect.asVoid),
);
