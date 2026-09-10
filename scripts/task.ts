import { Cause, Effect, Exit, Fiber, Match } from 'effect';

import { runCheck } from './task/check';
import { TaskCliError } from './task/errors';
import { TASK_HELP } from './task/help';
import { buildIced, runHot, runIced } from './task/iced';
import { prepareIced } from './task/iced-source';
import { runApi } from './task/misc';
import { runMonitor } from './task/monitor';
import { buildMpv } from './task/mpv';
import { runNativeRegression } from './task/native-regression';
import { parseCli } from './task/parse';
import { runFormat, runLint, runTypecheck } from './task/quality';
import { runRust } from './task/rust';

const program = Effect.try({
  try: () => parseCli(process.argv.slice(2)),
  catch: (cause) =>
    new TaskCliError({
      message: `${cause instanceof Error ? cause.message : String(cause)}\n\n${TASK_HELP}`,
    }),
}).pipe(
  Effect.tap((task) =>
    task._tag === 'check' ||
    task._tag === 'rust' ||
    task._tag === 'iced' ||
    task._tag === 'icedHot' ||
    task._tag === 'icedBuild'
      ? prepareIced(null)
      : Effect.void,
  ),
  Effect.flatMap((task) =>
    Match.value(task).pipe(
      Match.when({ _tag: 'help' }, () => Effect.sync(() => console.log(TASK_HELP))),
      Match.when({ _tag: 'check' }, () => runCheck()),
      Match.when({ _tag: 'fmt' }, ({ check }) => runFormat(check)),
      Match.when({ _tag: 'lint' }, ({ fix }) => runLint(fix)),
      Match.when({ _tag: 'typecheck' }, () => runTypecheck()),
      Match.when({ _tag: 'rust' }, (task) => runRust(task)),
      Match.when({ _tag: 'iced' }, ({ smoke, release, embedded }) =>
        runIced(smoke, release, embedded, process.env),
      ),
      Match.when({ _tag: 'mpvBuild' }, ({ source }) => buildMpv(source)),
      Match.when({ _tag: 'icedHot' }, () => runHot(process.env)),
      Match.when({ _tag: 'icedBuild' }, ({ release }) => buildIced(release)),
      Match.when({ _tag: 'icedPrepare' }, ({ source }) => prepareIced(source)),
      Match.when({ _tag: 'icedRegress' }, (task) => runNativeRegression(task, process.env)),
      Match.when({ _tag: 'monitor' }, (task) => runMonitor(task)),
      Match.when({ _tag: 'api' }, () => runApi()),
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
