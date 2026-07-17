import { Cause, Effect, Exit, Fiber, Match } from 'effect';

import { BUILD_TIMEOUT_MS, REPO_ROOT } from './constants';
import { buildE2e } from './orchestrator/build';
import { cleanE2e } from './orchestrator/clean';
import { checkProductionIsolation } from './orchestrator/isolation';
import { runE2eSpecs } from './orchestrator/run';
import { parseCli } from './support/options';
import { runCommand } from './support/process';

const { command, options } = parseCli(process.argv.slice(2));
const baseEnv = { ...process.env };

const typecheckE2e = runCommand({
  command: 'bun',
  args: ['x', 'tsc', '--noEmit', '-p', 'e2e/tsconfig.json'],
  cwd: REPO_ROOT,
  env: baseEnv,
  timeoutMs: BUILD_TIMEOUT_MS,
});

const program = Match.value(command).pipe(
  Match.when('build', () => buildE2e(baseEnv).pipe(Effect.asVoid)),
  Match.when('clean', () => cleanE2e),
  Match.when('isolation', () => checkProductionIsolation(baseEnv)),
  Match.when('test', () => runE2eSpecs(options, baseEnv)),
  Match.when('typecheck', () => typecheckE2e.pipe(Effect.asVoid)),
  Match.when('verify', () =>
    Effect.gen(function* () {
      yield* typecheckE2e;
      yield* buildE2e(baseEnv);
      yield* runE2eSpecs(options, baseEnv);
      yield* checkProductionIsolation(baseEnv);
    }),
  ),
  Match.exhaustive,
);

const fiber = Effect.runFork(program);
let interrupted = false;
const interrupt = () => {
  interrupted = true;
  void Effect.runPromise(Fiber.interrupt(fiber));
};

process.once('SIGINT', interrupt);
process.once('SIGTERM', interrupt);

const exit = await Effect.runPromise(Fiber.await(fiber));
process.removeListener('SIGINT', interrupt);
process.removeListener('SIGTERM', interrupt);

if (Exit.isFailure(exit)) {
  console.error(interrupted ? 'Native E2E interrupted after cleanup.' : Cause.pretty(exit.cause));
  process.exitCode = interrupted ? 130 : 1;
}
