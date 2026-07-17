import { spawn, type ChildProcess } from 'node:child_process';
import { appendFile, mkdir } from 'node:fs/promises';
import path from 'node:path';

import { Effect } from 'effect';

import { E2eProcessError } from '../support/errors';
import { terminateProcessGroup } from '../support/process';

export interface DisplaySession {
  readonly display: string;
  readonly displayNumber?: number;
  readonly process?: ChildProcess;
  readonly flushLogs?: () => Promise<void>;
}

const startXvfb = (env: NodeJS.ProcessEnv, logPath: string) =>
  Effect.tryPromise({
    try: async () => {
      await mkdir(path.dirname(logPath), { recursive: true });
      const child = spawn(
        'Xvfb',
        ['-displayfd', '1', '-screen', '0', '1600x900x24', '-nolisten', 'tcp'],
        {
          detached: true,
          env,
          stdio: ['ignore', 'pipe', 'pipe'],
        },
      );
      let logWrite = Promise.resolve();
      let logError: unknown;
      const record = (chunk: Buffer) => {
        logWrite = logWrite
          .then(() => appendFile(logPath, chunk))
          .catch((error: unknown) => {
            logError ??= error;
          });
      };
      const flushLogs = async () => {
        await logWrite;
        if (logError) throw logError instanceof Error ? logError : new Error(String(logError));
      };
      child.stderr?.on('data', record);
      const displayNumber = await new Promise<number>((resolve, reject) => {
        let output = '';
        const timeout = setTimeout(
          () => reject(new Error('Xvfb display allocation timed out.')),
          15_000,
        );
        child.stdout?.on('data', (chunk: Buffer) => {
          record(chunk);
          output += chunk.toString();
          const line = output.match(/(?:^|\n)(\d+)\s*(?:\n|$)/);
          if (!line) return;
          clearTimeout(timeout);
          resolve(Number(line[1]));
        });
        child.once('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
        child.once('exit', (code) => {
          clearTimeout(timeout);
          reject(new Error(`Xvfb exited before allocating a display (${code ?? 'signal'}).`));
        });
      });
      await flushLogs();
      const session: DisplaySession = {
        display: `:${displayNumber}`,
        displayNumber,
        process: child,
        flushLogs,
      };
      return session;
    },
    catch: (cause) =>
      new E2eProcessError({
        command: 'Xvfb',
        exitCode: null,
        timedOut: false,
        message: cause instanceof Error ? cause.message : String(cause),
      }),
  });

export function displayResource(headed: boolean, env: NodeJS.ProcessEnv, logPath: string) {
  if (headed) {
    const display = env.DISPLAY;
    if (display) {
      const session: DisplaySession = { display };
      return Effect.succeed(session);
    }
    return Effect.fail(
      new E2eProcessError({
        command: 'headed display',
        exitCode: null,
        timedOut: false,
        message: '--headed requires DISPLAY to be set.',
      }),
    );
  }

  return Effect.acquireRelease(startXvfb(env, logPath), (session) =>
    Effect.tryPromise({
      try: async () => {
        await terminateProcessGroup(session.process?.pid);
        await session.flushLogs?.();
      },
      catch: (cause) =>
        new E2eProcessError({
          command: 'Xvfb cleanup',
          exitCode: session.process?.exitCode ?? null,
          timedOut: false,
          message: `Could not finish Xvfb cleanup: ${String(cause)}`,
        }),
    }).pipe(Effect.catch((error) => Effect.logError('Xvfb cleanup failed.', error))),
  );
}
