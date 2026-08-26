import { Effect } from 'effect';

import {
  formatCommand,
  lintCommand,
  pandaCodegenCommand,
  typecheckCommands,
  wasmBuildCommand,
} from './commands';
import { runCommand, runCommands } from './process';

export const runFormat = Effect.fn('task.format')((check: boolean) =>
  runCommand(formatCommand(check)).pipe(Effect.asVoid),
);

export const runLint = Effect.fn('task.lint')((fix: boolean) =>
  runCommand(lintCommand(fix)).pipe(Effect.asVoid),
);

export const runTypecheck = Effect.fn('task.typecheck')(function* (skipSetup: boolean) {
  if (!skipSetup) {
    yield* runCommands([pandaCodegenCommand(), wasmBuildCommand('--dev')]);
  }
  yield* runCommands(typecheckCommands());
});
