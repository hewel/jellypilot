import { Effect } from 'effect';

import { command, e2eSetupCommands } from './commands';
import type { E2eSubcommand } from './parse';
import { runCommand } from './process';

export interface E2eTask {
  readonly action: E2eSubcommand;
  readonly args: readonly string[];
  readonly skipSetup: boolean;
}

export const runE2e = Effect.fn('task.e2e')(function* (task: E2eTask) {
  if (!task.skipSetup) for (const step of e2eSetupCommands(task.action)) yield* runCommand(step);
  yield* runCommand(command('bun', ['x', 'tsx', 'e2e/cli.ts', task.action, ...task.args]));
});
