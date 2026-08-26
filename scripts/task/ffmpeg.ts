import { Effect } from 'effect';

import { command } from './commands';
import { runCommand } from './process';

export interface FfmpegOptions {
  readonly target?: string;
  readonly verify: boolean;
}

export const runFfmpeg = Effect.fn('task.ffmpeg')((options: FfmpegOptions) =>
  runCommand(
    command('bun', [
      'scripts/prepare-ffmpeg-sidecar.ts',
      ...(options.verify ? ['--verify'] : []),
      ...(options.target === undefined ? [] : ['--target', options.target]),
    ]),
  ).pipe(Effect.asVoid),
);
