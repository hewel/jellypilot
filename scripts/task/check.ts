import { Effect } from 'effect';

import type { CommandSpec } from './commands';
import {
  command,
  formatCommand,
  lintCommand,
  pandaCodegenCommand,
  rustClippyWorkspaceCommands,
  rustFormatCommand,
  typecheckCommands,
  wasmBuildCommand,
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

const runDependentSteps = Effect.fn('task.check.dependent')(function* (
  steps: readonly BufferedStep[],
) {
  const results: StepResult[] = [];
  for (const step of steps) {
    const result = yield* runBufferedStep(step);
    results.push(result);
    if (result.failed) break;
  }
  return results;
});

function printResults(results: readonly StepResult[]): void {
  for (const result of results) {
    const output = result.output.trimEnd();
    console.log(`\n=== ${result.label} [${result.failed ? 'FAILED' : 'OK'}] ===`);
    if (output.length > 0) console.log(output);
  }
}

export const runCheck = Effect.fn('task.check')(function* () {
  const independentSteps: readonly BufferedStep[] = [
    { label: 'fmt --check', commands: [formatCommand(true)] },
    { label: 'lint', commands: [lintCommand(false)] },
    {
      label: 'check:effect-rules',
      commands: [command('bun', ['scripts/check-no-switch.ts'])],
    },
    {
      label: 'check:styling-boundaries',
      commands: [command('bun', ['scripts/check-styling-boundaries.ts'])],
    },
    { label: 'rust fmt --check', commands: [rustFormatCommand(true)] },
    { label: 'rust clippy', commands: rustClippyWorkspaceCommands() },
  ];
  const typecheckSteps: readonly BufferedStep[] = [
    { label: 'panda codegen', commands: [pandaCodegenCommand()] },
    { label: 'wasm build --dev', commands: [wasmBuildCommand('--dev')] },
    { label: 'typecheck', commands: typecheckCommands() },
  ];

  const [independentResults, dependentResults] = yield* Effect.all(
    [
      Effect.all(independentSteps.map(runBufferedStep), { concurrency: 'unbounded' }),
      runDependentSteps(typecheckSteps),
    ],
    { concurrency: 'unbounded' },
  );
  const results = [...independentResults, ...dependentResults];
  yield* Effect.sync(() => printResults(results));

  const failedSteps = results.filter((result) => result.failed).map((result) => result.label);
  if (failedSteps.length > 0) {
    return yield* new TaskCheckError({
      failedSteps,
      message: `Check failed: ${failedSteps.join(', ')}`,
    });
  }
});
