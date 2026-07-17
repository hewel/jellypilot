import { randomUUID } from 'node:crypto';
import { access, mkdir, readdir, writeFile } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';

import { Cause, Effect, Exit } from 'effect';

import {
  BINARY_PATH,
  E2E_ROOT,
  REPO_ROOT,
  RUNS_ROOT,
  SPEC_PROCESS_TIMEOUT_MS,
  SPEC_TIMEOUT_MS,
} from '../constants';
import { E2eRunError } from '../support/errors';
import type { E2eProcessError } from '../support/errors';
import { retainFailureEvidence } from '../support/evidence';
import type { BuildManifest } from '../support/fingerprint';
import { verifyBuildManifest } from '../support/fingerprint';
import type { TestOptions } from '../support/options';
import { ensureOwnedDirectory, removeOwnedDirectory } from '../support/ownership';
import { preflight } from '../support/preflight';
import { processGroupIsGone, runCommand } from '../support/process';
import { displayResource } from './display';

export interface RunSandbox {
  readonly root: string;
  readonly home: string;
  readonly temporary: string;
  readonly logs: string;
  readonly xdg: {
    readonly cache: string;
    readonly config: string;
    readonly data: string;
    readonly runtime: string;
    readonly state: string;
  };
}

async function freePort(): Promise<number> {
  const server = net.createServer();
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') {
    server.close();
    throw new Error('Could not allocate an embedded WebDriver port.');
  }
  await new Promise<void>((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
  return address.port;
}

async function pathIsMissing(target: string): Promise<boolean> {
  try {
    await access(target);
    return false;
  } catch (error) {
    if (
      typeof error === 'object' &&
      error !== null &&
      'code' in error &&
      (error.code === 'ENOENT' || error.code === 'ENOTDIR')
    ) {
      return true;
    }
    throw error;
  }
}

async function portIsClosed(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: '127.0.0.1', port });
    socket.setTimeout(500);
    socket.once('connect', () => {
      socket.destroy();
      resolve(false);
    });
    socket.once('error', () => resolve(true));
    socket.once('timeout', () => {
      socket.destroy();
      resolve(true);
    });
  });
}

export function createRunSandboxLayout(spec: string): RunSandbox {
  const identifier = `${Date.now()}-${path.basename(spec, '.e2e.ts')}-${randomUUID().slice(0, 8)}`;
  const root = path.join(RUNS_ROOT, identifier);
  return {
    root,
    home: path.join(root, 'home'),
    temporary: path.join(root, 'tmp'),
    logs: path.join(root, 'logs'),
    xdg: {
      cache: path.join(root, 'xdg/cache'),
      config: path.join(root, 'xdg/config'),
      data: path.join(root, 'xdg/data'),
      runtime: path.join(root, 'xdg/runtime'),
      state: path.join(root, 'xdg/state'),
    },
  };
}

const createSandbox = Effect.fn('e2e.createSandbox')(function* (sandbox: RunSandbox) {
  yield* ensureOwnedDirectory(E2E_ROOT);
  yield* ensureOwnedDirectory(RUNS_ROOT);
  yield* ensureOwnedDirectory(sandbox.root);
  yield* Effect.tryPromise({
    try: () =>
      Promise.all([
        mkdir(sandbox.home, { recursive: true }),
        mkdir(sandbox.temporary, { recursive: true }),
        mkdir(sandbox.logs, { recursive: true }),
        ...Object.values(sandbox.xdg).map((directory) => mkdir(directory, { recursive: true })),
      ]),
    catch: (cause) => new E2eRunError({ message: 'Could not create an E2E sandbox.', cause }),
  });
  return sandbox;
});

async function discoverSpecs(selected?: string): Promise<string[]> {
  const specsRoot = path.join(REPO_ROOT, 'e2e/specs');
  const controlledFailure = path.join(REPO_ROOT, 'e2e/fixtures/controlled-failure.e2e.ts');
  if (selected) {
    const absolute = path.resolve(REPO_ROOT, selected);
    const permanentSpec =
      absolute.startsWith(`${specsRoot}${path.sep}`) && absolute.endsWith('.e2e.ts');
    if (!permanentSpec && absolute !== controlledFailure) {
      throw new Error(
        '--spec must name a permanent e2e/specs/*.e2e.ts file or the controlled failure fixture.',
      );
    }
    return [absolute];
  }
  const entries = await readdir(specsRoot);
  return entries
    .filter((entry) => entry.endsWith('.e2e.ts'))
    .toSorted()
    .map((entry) => path.join(specsRoot, entry));
}

function sandboxEnvironment(
  baseEnv: NodeJS.ProcessEnv,
  sandbox: RunSandbox,
  display: string,
  port: number,
): NodeJS.ProcessEnv {
  return {
    ...baseEnv,
    DISPLAY: display,
    HOME: sandbox.home,
    TMPDIR: sandbox.temporary,
    XDG_CACHE_HOME: sandbox.xdg.cache,
    XDG_CONFIG_HOME: sandbox.xdg.config,
    XDG_DATA_HOME: sandbox.xdg.data,
    XDG_RUNTIME_DIR: sandbox.xdg.runtime,
    XDG_STATE_HOME: sandbox.xdg.state,
    JELLYPILOT_E2E_BINARY: BINARY_PATH,
    JELLYPILOT_E2E_LOG_DIR: sandbox.logs,
    JELLYPILOT_E2E_PORT: String(port),
    TAURI_WEBDRIVER_PORT: String(port),
  };
}

const writeTeardown = (sandbox: RunSandbox, diagnostics: Record<string, unknown>) =>
  Effect.tryPromise({
    try: () =>
      writeFile(
        path.join(sandbox.logs, 'teardown.json'),
        `${JSON.stringify(diagnostics, null, 2)}\n`,
      ),
    catch: (cause) => new E2eRunError({ message: 'Could not write teardown diagnostics.', cause }),
  });

interface TeardownTargets {
  readonly displayNumber?: number;
  readonly port: number;
  readonly processGroupPids: readonly number[];
}

export const verifyTeardownTargets = Effect.fn('e2e.verifyTeardownTargets')(
  (targets: TeardownTargets) =>
    Effect.tryPromise({
      try: async () => {
        const portClosed = await portIsClosed(targets.port);
        if (!portClosed) throw new Error(`Embedded WebDriver port ${targets.port} remained open.`);
        const liveProcessGroup = targets.processGroupPids.find((pid) => !processGroupIsGone(pid));
        if (liveProcessGroup) throw new Error(`Owned process group ${liveProcessGroup} remained.`);
        if (targets.displayNumber !== undefined) {
          const socketPath = `/tmp/.X11-unix/X${targets.displayNumber}`;
          if (!(await pathIsMissing(socketPath))) {
            throw new Error(`Xvfb socket ${socketPath} remained after teardown.`);
          }
        }
        return { portClosed };
      },
      catch: (cause) => new E2eRunError({ message: 'E2E teardown verification failed.', cause }),
    }),
);

function restoreExit<A, E>(exit: Exit.Exit<A, E>): Effect.Effect<A, E> {
  return Exit.isSuccess(exit) ? Effect.succeed(exit.value) : Effect.failCause(exit.cause);
}

const runOneSpec = Effect.fn('e2e.runOneSpec')(function* (
  spec: string,
  options: TestOptions,
  manifest: BuildManifest,
  baseEnv: NodeJS.ProcessEnv,
) {
  const sandbox = createRunSandboxLayout(spec);
  const scopedRun = Effect.scoped(
    Effect.acquireRelease(createSandbox(sandbox), () =>
      removeOwnedDirectory(sandbox.root).pipe(
        Effect.catch((error) => Effect.logError('Could not release the E2E sandbox.', error)),
      ),
    ).pipe(
      Effect.flatMap(() =>
        Effect.gen(function* () {
          const port = yield* Effect.tryPromise({
            try: freePort,
            catch: (cause) =>
              new E2eRunError({ message: 'Could not allocate an E2E port.', cause }),
          });
          const processGroupPids: number[] = [];
          let displayNumber: number | undefined;
          let displayName: string | undefined;
          const executionExit = yield* Effect.exit(
            Effect.scoped(
              Effect.gen(function* () {
                const display = yield* displayResource(
                  options.headed,
                  baseEnv,
                  path.join(sandbox.logs, 'xvfb.log'),
                );
                displayNumber = display.displayNumber;
                displayName = display.display;
                if (display.process?.pid) processGroupPids.push(display.process.pid);
                yield* runCommand({
                  command: path.join(REPO_ROOT, 'node_modules/.bin/wdio'),
                  args: ['run', 'e2e/wdio.conf.ts', '--spec', spec],
                  cwd: REPO_ROOT,
                  env: sandboxEnvironment(baseEnv, sandbox, display.display, port),
                  timeoutMs: SPEC_PROCESS_TIMEOUT_MS,
                  logPath: path.join(sandbox.logs, 'runner.log'),
                  onSpawn: (pid) => processGroupPids.push(pid),
                });
              }),
            ),
          );
          const teardownExit = yield* Effect.exit(
            verifyTeardownTargets({ displayNumber, port, processGroupPids }).pipe(Effect.asVoid),
          );
          const finalExit: Exit.Exit<void, E2eRunError | E2eProcessError> = Exit.isFailure(
            teardownExit,
          )
            ? teardownExit
            : executionExit;
          yield* writeTeardown(sandbox, {
            status: Exit.isSuccess(finalExit) ? 'passed' : 'failed',
            port,
            display: displayName,
            processGroupPids,
            message: Exit.isFailure(finalExit) ? Cause.pretty(finalExit.cause) : undefined,
          });
          if (Exit.isFailure(finalExit)) {
            yield* retainFailureEvidence(
              sandbox.root,
              spec,
              Cause.pretty(finalExit.cause),
              manifest,
            ).pipe(
              Effect.catch((error) => Effect.logError('Failure evidence was not retained.', error)),
            );
          }
          return yield* restoreExit(finalExit);
        }),
      ),
    ),
  );

  const runExit = yield* Effect.exit(
    scopedRun.pipe(
      Effect.timeoutOrElse({
        duration: SPEC_TIMEOUT_MS,
        orElse: () =>
          Effect.fail(
            new E2eRunError({
              message: `Native E2E spec exceeded the five-minute limit, including teardown: ${spec}`,
            }),
          ),
      }),
    ),
  );
  const sandboxGone = yield* Effect.tryPromise({
    try: () => pathIsMissing(sandbox.root),
    catch: (cause) => new E2eRunError({ message: 'Could not verify sandbox cleanup.', cause }),
  });
  if (!sandboxGone) {
    return yield* new E2eRunError({ message: `Run-owned sandbox remained: ${sandbox.root}` });
  }
  return yield* restoreExit(runExit);
});

export const runE2eSpecs = Effect.fn('e2e.runSpecs')(function* (
  options: TestOptions,
  baseEnv: NodeJS.ProcessEnv,
) {
  yield* preflight();
  const manifest = yield* verifyBuildManifest();
  const specs = yield* Effect.tryPromise({
    try: () => discoverSpecs(options.spec),
    catch: (cause) => new E2eRunError({ message: 'Could not select native E2E specs.', cause }),
  });
  for (const spec of specs) yield* runOneSpec(spec, options, manifest, baseEnv);
});
