import { spawn, type ChildProcess } from 'node:child_process';
import { appendFile, mkdir } from 'node:fs/promises';
import path from 'node:path';

import { Effect } from 'effect';

import { E2eProcessError } from './errors';

export interface CommandRequest {
  readonly command: string;
  readonly args?: readonly string[];
  readonly cwd: string;
  readonly env: NodeJS.ProcessEnv;
  readonly timeoutMs: number;
  readonly logPath?: string;
  readonly acceptNonZero?: boolean;
  readonly onSpawn?: (pid: number) => void;
}

export interface CommandResult {
  readonly exitCode: number;
  readonly output: string;
}

interface OwnedProcess {
  readonly child: ChildProcess;
  readonly request: CommandRequest;
  output: string;
  logWrite: Promise<void>;
  logError?: unknown;
}

export const terminateProcessGroup = async (pid: number | undefined): Promise<void> => {
  if (!pid) return;

  try {
    process.kill(-pid, 'SIGTERM');
  } catch {
    return;
  }

  await new Promise((resolve) => setTimeout(resolve, 1000));
  try {
    process.kill(-pid, 0);
    process.kill(-pid, 'SIGKILL');
  } catch {
    // The exact owned process group already exited.
  }
};

export function processGroupIsGone(pid: number): boolean {
  try {
    process.kill(-pid, 0);
    return false;
  } catch (error) {
    if (typeof error === 'object' && error !== null && 'code' in error) {
      if (error.code === 'ESRCH') return true;
      if (error.code === 'EPERM') return false;
    }
    throw error;
  }
}

const terminateOwnedProcess = (owned: OwnedProcess): Promise<void> =>
  terminateProcessGroup(owned.child.pid);

const spawnOwnedProcess = (request: CommandRequest) =>
  Effect.tryPromise({
    try: async () => {
      if (request.logPath) await mkdir(path.dirname(request.logPath), { recursive: true });
      const child = spawn(request.command, [...(request.args ?? [])], {
        cwd: request.cwd,
        detached: true,
        env: request.env,
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      const owned: OwnedProcess = { child, request, output: '', logWrite: Promise.resolve() };
      if (child.pid) request.onSpawn?.(child.pid);
      const record = (chunk: Buffer) => {
        const text = chunk.toString();
        owned.output = `${owned.output}${text}`.slice(-2_000_000);
        const logPath = request.logPath;
        if (logPath) {
          owned.logWrite = owned.logWrite
            .then(() => appendFile(logPath, text))
            .catch((error: unknown) => {
              owned.logError ??= error;
            });
        }
      };
      child.stdout?.on('data', record);
      child.stderr?.on('data', record);
      await new Promise<void>((resolve, reject) => {
        child.once('spawn', resolve);
        child.once('error', reject);
      });
      return owned;
    },
    catch: (cause) =>
      new E2eProcessError({
        command: request.command,
        exitCode: null,
        timedOut: false,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

const awaitOwnedProcess = (owned: OwnedProcess) =>
  Effect.tryPromise({
    try: () =>
      new Promise<CommandResult>((resolve, reject) => {
        let timedOut = false;
        const timeout = setTimeout(() => {
          timedOut = true;
          void terminateOwnedProcess(owned);
        }, owned.request.timeoutMs);

        owned.child.once('exit', async (exitCode) => {
          clearTimeout(timeout);
          await owned.logWrite;
          if (owned.logError) {
            reject(
              new E2eProcessError({
                command: owned.request.command,
                exitCode,
                timedOut,
                message: `Could not write process logs: ${String(owned.logError)}`,
              }),
            );
            return;
          }
          if ((exitCode === 0 || (owned.request.acceptNonZero && exitCode !== null)) && !timedOut) {
            resolve({ exitCode, output: owned.output });
            return;
          }
          reject(
            new E2eProcessError({
              command: [owned.request.command, ...(owned.request.args ?? [])].join(' '),
              exitCode,
              timedOut,
              message: timedOut
                ? `Command exceeded ${owned.request.timeoutMs}ms.\n${owned.output}`
                : `Command exited with ${exitCode ?? 'a signal'}.\n${owned.output}`,
            }),
          );
        });
        owned.child.once('error', reject);
      }),
    catch: (cause) =>
      cause instanceof E2eProcessError
        ? cause
        : new E2eProcessError({
            command: owned.request.command,
            exitCode: owned.child.exitCode,
            timedOut: false,
            message: cause instanceof Error ? cause.message : String(cause),
          }),
  });

export const runCommand = Effect.fn('e2e.runCommand')((request: CommandRequest) =>
  Effect.scoped(
    Effect.acquireRelease(spawnOwnedProcess(request), (owned) =>
      Effect.promise(() => terminateOwnedProcess(owned)),
    ).pipe(Effect.flatMap(awaitOwnedProcess)),
  ),
);
