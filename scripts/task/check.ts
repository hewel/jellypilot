import { Effect } from 'effect';

import type { CommandSpec } from './commands';
import {
  formatCommand,
  lintCommand,
  rustClippyWorkspaceCommands,
  rustFormatCommand,
  scriptTestCommands,
  typecheckCommands,
} from './commands';
import type { TaskProcessError } from './errors';
import { TaskCheckError } from './errors';
import type { CommandResult } from './process';
import { formatCommand as formatInvocation, runCommand } from './process';

interface BufferedStep {
  readonly label: string;
  readonly commands: readonly CommandSpec[];
}

interface StepResult {
  readonly label: string;
  readonly output: string;
  readonly failed: boolean;
}

type BufferedCommandResult =
  | { readonly _tag: 'failure'; readonly error: TaskProcessError }
  | { readonly _tag: 'success'; readonly value: CommandResult };

const runBufferedStep = Effect.fn('task.check.step')(function* (step: BufferedStep) {
  const output: string[] = [];
  let failed = false;
  for (const request of step.commands) {
    output.push(`$ ${formatInvocation(request)}\n`);
    const result = yield* runCommand({ ...request, buffered: true, acceptNonZero: true }).pipe(
      Effect.match({
        onFailure: (error): BufferedCommandResult => ({ _tag: 'failure', error }),
        onSuccess: (value): BufferedCommandResult => ({ _tag: 'success', value }),
      }),
    );
    if (result._tag === 'failure') {
      output.push(`${result.error.message}\n`);
      failed = true;
      break;
    }
    output.push(result.value.stdout, result.value.stderr);
    if (result.value.exitCode !== 0) {
      failed = true;
      break;
    }
  }
  return { label: step.label, output: output.join(''), failed } satisfies StepResult;
});

function printResults(results: readonly StepResult[]): void {
  for (const result of results) {
    const output = result.output.trimEnd();
    console.log(`\n=== ${result.label} [${result.failed ? 'FAILED' : 'OK'}] ===`);
    if (output.length > 0) console.log(output);
  }
}

export const runCheck = Effect.fn('task.check')(function* () {
  const steps: readonly BufferedStep[] = [
    { label: 'fmt --check', commands: [formatCommand(true)] },
    { label: 'lint', commands: [lintCommand(false)] },
    { label: 'typecheck', commands: typecheckCommands() },
    { label: 'script tests', commands: scriptTestCommands() },
    { label: 'rust fmt --check', commands: [rustFormatCommand(true)] },
    { label: 'rust clippy', commands: rustClippyWorkspaceCommands() },
  ];

  const results = yield* Effect.all(steps.map(runBufferedStep), { concurrency: 'unbounded' });
  yield* Effect.sync(() => printResults([...results]));

  const failedSteps = results.filter((result) => result.failed).map((result) => result.label);
  if (failedSteps.length > 0) {
    return yield* new TaskCheckError({
      failedSteps,
      message: `Check failed: ${failedSteps.join(', ')}`,
    });
  }
});
