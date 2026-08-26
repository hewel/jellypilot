import { Effect } from 'effect';

import { command } from './commands';
import { runCommand } from './process';

export const runGtk = Effect.fn('task.gtk')((smoke: boolean) =>
  runCommand(
    command('cargo', [
      'run',
      '--manifest-path',
      'Cargo.toml',
      '--package',
      'jellypilot-gtk',
      ...(smoke ? ['--', '--smoke-test'] : []),
    ]),
  ).pipe(Effect.asVoid),
);
