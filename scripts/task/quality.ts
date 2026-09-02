import { Effect } from 'effect';

import { formatCommand, lintCommand, typecheckCommands } from './commands';
import { runCommand, runCommands } from './process';

export const runFormat = Effect.fn('task.format')((check: boolean) =>
  runCommand(formatCommand(check)).pipe(Effect.asVoid),
);

export const runLint = Effect.fn('task.lint')((fix: boolean) =>
  runCommand(lintCommand(fix)).pipe(Effect.asVoid),
);

export const runTypecheck = Effect.fn('task.typecheck')(() =>
  runCommands(typecheckCommands()).pipe(Effect.asVoid),
);
