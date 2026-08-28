import { Cause, Effect, Exit, Fiber, Match } from 'effect';

import { runCheck } from './task/check';
import { runBuild, runDev, runPreview, runTest } from './task/dev';
import { runE2e } from './task/e2e';
import { TaskCliError } from './task/errors';
import { runFfmpeg } from './task/ffmpeg';
import { TASK_HELP } from './task/help';
import { runIced } from './task/iced';
import { runApi, runPanda, runReview } from './task/misc';
import { parseCli } from './task/parse';
import { runFormat, runLint, runTypecheck } from './task/quality';
import { runRust } from './task/rust';
import { runWasm } from './task/wasm';

const program = Effect.try({
  try: () => parseCli(process.argv.slice(2)),
  catch: (cause) =>
    new TaskCliError({
      message: `${cause instanceof Error ? cause.message : String(cause)}\n\n${TASK_HELP}`,
    }),
}).pipe(
  Effect.flatMap((task) =>
    Match.value(task).pipe(
      Match.when({ _tag: 'help' }, () => Effect.sync(() => console.log(TASK_HELP))),
      Match.when({ _tag: 'dev' }, ({ rsdoctor, skipSetup }) => runDev({ rsdoctor, skipSetup })),
      Match.when({ _tag: 'build' }, ({ rsdoctor, skipSetup }) => runBuild({ rsdoctor, skipSetup })),
      Match.when({ _tag: 'preview' }, () => runPreview()),
      Match.when({ _tag: 'test' }, ({ all, skipSetup, watch }) =>
        runTest({ all, skipSetup, watch }),
      ),
      Match.when({ _tag: 'check' }, () => runCheck()),
      Match.when({ _tag: 'fmt' }, ({ check }) => runFormat(check)),
      Match.when({ _tag: 'lint' }, ({ fix }) => runLint(fix)),
      Match.when({ _tag: 'typecheck' }, ({ skipSetup }) => runTypecheck(skipSetup)),
      Match.when({ _tag: 'rust' }, (task) => runRust(task)),
      Match.when({ _tag: 'wasm' }, (task) => runWasm(task)),
      Match.when({ _tag: 'ffmpeg' }, ({ target, verify }) => runFfmpeg({ target, verify })),
      Match.when({ _tag: 'iced' }, ({ smoke }) => runIced(smoke)),
      Match.when({ _tag: 'e2e' }, ({ action, args, skipSetup }) =>
        runE2e({ action, args, skipSetup }),
      ),
      Match.when({ _tag: 'api' }, () => runApi()),
      Match.when({ _tag: 'panda' }, () => runPanda()),
      Match.when({ _tag: 'review' }, ({ action, args }) => runReview(action, args)),
      Match.exhaustive,
    ),
  ),
  Effect.catchTag('TaskCliError', (error) =>
    Effect.sync(() => {
      console.error(error.message);
      process.exitCode = 1;
    }),
  ),
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
  console.error(interrupted ? 'Task interrupted after cleanup.' : Cause.pretty(exit.cause));
  process.exitCode = interrupted ? 130 : 1;
}
