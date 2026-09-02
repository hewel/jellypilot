import { spawn, type ChildProcess } from 'node:child_process';

import { Effect } from 'effect';

import type { CommandSpec } from './commands';
import { MAX_CAPTURED_OUTPUT_BYTES, REPO_ROOT } from './constants';
import { TaskProcessError } from './errors';

export interface CommandRequest extends CommandSpec {
  readonly buffered?: boolean;
  readonly acceptNonZero?: boolean;
}

export interface CommandResult {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
}

interface OwnedProcess {
  readonly child: ChildProcess;
  readonly request: CommandRequest;
  stdout: string;
  stderr: string;
}

function appendCaptured(current: string, chunk: string): string {
  return `${current}${chunk}`.slice(-MAX_CAPTURED_OUTPUT_BYTES);
}

export function formatCommand(request: CommandSpec): string {
  return [request.command, ...request.args].join(' ');
}

const terminateProcessGroup = async (pid: number | undefined): Promise<void> => {
  if (pid === undefined) return;
  try {
    process.kill(-pid, 'SIGTERM');
  } catch {
    return;
  }

  const deadline = Date.now() + 1000;
  while (Date.now() < deadline) {
    try {
      process.kill(-pid, 0);
    } catch {
      return;
    }
    const delay = Promise.withResolvers<void>();
    setTimeout(delay.resolve, 25);
    await delay.promise;
  }

  try {
    process.kill(-pid, 'SIGKILL');
  } catch {
    // The exact owned process group already exited.
  }
};

const spawnOwnedProcess = (request: CommandRequest) =>
  Effect.tryPromise({
    try: () => {
      const spawned = Promise.withResolvers<OwnedProcess>();
      const child = spawn(request.command, [...request.args], {
        cwd: REPO_ROOT,
        detached: true,
        env: { ...process.env, ...request.env },
        stdio: ['inherit', 'pipe', 'pipe'],
      });
      const owned: OwnedProcess = { child, request, stdout: '', stderr: '' };
      child.stdout?.on('data', (chunk: Buffer) => {
        const text = chunk.toString();
        owned.stdout = appendCaptured(owned.stdout, text);
        if (!request.buffered) process.stdout.write(text);
      });
      child.stderr?.on('data', (chunk: Buffer) => {
        const text = chunk.toString();
        owned.stderr = appendCaptured(owned.stderr, text);
        if (!request.buffered) process.stderr.write(text);
      });
      child.once('spawn', () => spawned.resolve(owned));
      child.once('error', spawned.reject);
      return spawned.promise;
    },
    catch: (cause) =>
      new TaskProcessError({
        command: formatCommand(request),
        exitCode: null,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

const awaitOwnedProcess = (owned: OwnedProcess) =>
  Effect.tryPromise({
    try: () => {
      const exited = Promise.withResolvers<CommandResult>();
      owned.child.once('exit', (exitCode) => {
        if (exitCode !== null && (exitCode === 0 || owned.request.acceptNonZero)) {
          exited.resolve({ exitCode, stdout: owned.stdout, stderr: owned.stderr });
          return;
        }
        exited.reject(
          new TaskProcessError({
            command: formatCommand(owned.request),
            exitCode,
            message: `Command exited with ${exitCode ?? 'a signal'}.`,
          }),
        );
      });
      owned.child.once('error', exited.reject);
      return exited.promise;
    },
    catch: (cause) =>
      cause instanceof TaskProcessError
        ? cause
        : new TaskProcessError({
            command: formatCommand(owned.request),
            exitCode: owned.child.exitCode,
            message: cause instanceof Error ? cause.message : String(cause),
          }),
  });

export const runCommand = Effect.fn('task.runCommand')((request: CommandRequest) =>
  Effect.scoped(
    Effect.acquireRelease(spawnOwnedProcess(request), (owned) =>
      Effect.promise(() => terminateProcessGroup(owned.child.pid)),
    ).pipe(Effect.flatMap(awaitOwnedProcess)),
  ),
);

export const runCommands = Effect.fn('task.runCommands')(function* (
  requests: readonly CommandSpec[],
) {
  for (const request of requests) yield* runCommand(request);
});
