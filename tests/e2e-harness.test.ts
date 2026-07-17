import { access, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import net from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';

import { expect, test } from '@rstest/core';
import { Effect, Exit, Fiber } from 'effect';

import { displayResource } from '../e2e/orchestrator/display';
import { createRunSandboxLayout, verifyTeardownTargets } from '../e2e/orchestrator/run';
import {
  evidenceTextIsSafe,
  isRetainableEvidenceFile,
  redactEvidenceText,
  selectStaleFailures,
} from '../e2e/support/evidence';
import { computeAppFingerprintAt } from '../e2e/support/fingerprint';
import { parseCli } from '../e2e/support/options';
import {
  ensureOwnedDirectory,
  removeOwnedDirectory,
  withOwnedDirectory,
} from '../e2e/support/ownership';
import { validateToolVersions, versionAtLeast } from '../e2e/support/preflight';
import { processGroupIsGone, runCommand } from '../e2e/support/process';

async function reservePort(): Promise<number> {
  const server = net.createServer();
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Could not reserve a test port.');
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

async function portIsOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: '127.0.0.1', port });
    socket.setTimeout(100);
    socket.once('connect', () => {
      socket.destroy();
      resolve(true);
    });
    socket.once('error', () => resolve(false));
    socket.once('timeout', () => {
      socket.destroy();
      resolve(false);
    });
  });
}

async function waitUntil(predicate: () => Promise<boolean>, timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return true;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  return false;
}

test('routes focused and headed native options', () => {
  expect(parseCli(['test', '--spec', 'e2e/specs/login-smoke.e2e.ts', '--headed'])).toEqual({
    command: 'test',
    options: { headed: true, spec: 'e2e/specs/login-smoke.e2e.ts' },
  });
  expect(() => parseCli(['test', '--spec'])).toThrow('--spec requires a path.');
});

test('classifies E2E tool preflight versions', () => {
  expect(versionAtLeast('v22.11.0', [22, 11, 0])).toBe(true);
  expect(versionAtLeast('rustc 1.84.1', [1, 85, 0])).toBe(false);
  expect(
    validateToolVersions('linux', { bun: '1.3.13', node: 'v24.0.0', rust: 'rustc 1.90.0' }),
  ).toBe('Native E2E requires Bun 1.3.14; found 1.3.13.');
});

test('fingerprints bundle inputs but reuses the binary for spec-only changes', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'jellypilot-fingerprint-'));
  try {
    await mkdir(path.join(root, 'src'), { recursive: true });
    await mkdir(path.join(root, 'e2e/app'), { recursive: true });
    await mkdir(path.join(root, 'e2e/specs'), { recursive: true });
    await mkdir(path.join(root, 'styled-system'), { recursive: true });
    await writeFile(path.join(root, 'src/app.ts'), 'export const app = 1;\n');
    await writeFile(path.join(root, 'e2e/app/bridge.ts'), 'export const bridge = 1;\n');
    await writeFile(path.join(root, 'e2e/specs/example.e2e.ts'), 'test(1);\n');
    await writeFile(path.join(root, 'styled-system/tokens.mjs'), 'export const token = 1;\n');
    const initial = await computeAppFingerprintAt(root);

    await writeFile(path.join(root, 'e2e/specs/example.e2e.ts'), 'test(2);\n');
    expect(await computeAppFingerprintAt(root)).toBe(initial);

    await writeFile(path.join(root, 'e2e/app/bridge.ts'), 'export const bridge = 2;\n');
    expect(await computeAppFingerprintAt(root)).not.toBe(initial);

    await writeFile(path.join(root, 'e2e/app/bridge.ts'), 'export const bridge = 1;\n');
    await writeFile(path.join(root, 'styled-system/tokens.mjs'), 'export const token = 2;\n');
    expect(await computeAppFingerprintAt(root)).not.toBe(initial);
  } finally {
    await rm(root, { force: true, recursive: true });
  }
});

test('creates distinct storage layouts for sequential native specs', () => {
  const first = createRunSandboxLayout('e2e/specs/login-smoke.e2e.ts');
  const second = createRunSandboxLayout('e2e/specs/login-smoke.e2e.ts');
  expect(second.root).not.toBe(first.root);
  expect(second.home).not.toBe(first.home);
  expect(second.temporary).not.toBe(first.temporary);
  expect(second.xdg).not.toEqual(first.xdg);
});

test('ownership guards refuse unrelated directories and removal is idempotent', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'jellypilot-ownership-'));
  const unowned = path.join(root, 'unowned');
  const owned = path.join(root, 'owned');
  try {
    await mkdir(unowned);
    expect(Exit.isFailure(await Effect.runPromiseExit(ensureOwnedDirectory(unowned)))).toBe(true);
    await Effect.runPromise(ensureOwnedDirectory(owned));
    await Effect.runPromise(removeOwnedDirectory(owned));
    await Effect.runPromise(removeOwnedDirectory(owned));
  } finally {
    await rm(root, { force: true, recursive: true });
  }
});

test('redacts credentials before evidence retention and chooses only bundles older than five', () => {
  const redacted = redactEvidenceText(
    'password="not-a-secret" authorization=Bearer-secret {"password":"different-secret"} https://user:secret@example.test',
  );
  expect(evidenceTextIsSafe(redacted)).toBe(true);
  expect(redacted).not.toContain('not-a-secret');
  expect(redacted).not.toContain('different-secret');
  expect(isRetainableEvidenceFile('runner.log')).toBe(true);
  expect(isRetainableEvidenceFile('page-source.html')).toBe(false);
  expect(isRetainableEvidenceFile('environment.json')).toBe(false);
  expect(selectStaleFailures([1, 2, 3, 4, 5, 6].map((modified) => ({ modified })))).toEqual([
    { modified: 1 },
  ]);
});

test('classifies an owned command timeout and releases its process group', async () => {
  let processGroupPid: number | undefined;
  const exit = await Effect.runPromiseExit(
    runCommand({
      command: process.execPath,
      args: ['-e', 'setTimeout(() => {}, 10_000)'],
      cwd: process.cwd(),
      env: { ...process.env },
      timeoutMs: 20,
      onSpawn: (pid) => {
        processGroupPid = pid;
      },
    }),
  );
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const reason = exit.cause.reasons[0];
    expect(reason?._tag).toBe('Fail');
    if (reason?._tag === 'Fail') expect(reason.error.timedOut).toBe(true);
  }
  expect(processGroupPid).toBeDefined();
  if (processGroupPid) expect(processGroupIsGone(processGroupPid)).toBe(true);
});

test('teardown verification rejects an occupied port and passes after release', async () => {
  const server = net.createServer();
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  expect(address && typeof address !== 'string').toBe(true);
  if (!address || typeof address === 'string') return;

  const occupied = await Effect.runPromiseExit(
    verifyTeardownTargets({ port: address.port, processGroupPids: [] }),
  );
  expect(Exit.isFailure(occupied)).toBe(true);

  await new Promise<void>((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
  await expect(
    Effect.runPromise(verifyTeardownTargets({ port: address.port, processGroupPids: [] })),
  ).resolves.toEqual({ portClosed: true });
});

test('interrupts scoped process, display, port, and storage resources in cleanup order', async () => {
  const parent = await mkdtemp(path.join(tmpdir(), 'jellypilot-interrupt-'));
  const owned = path.join(parent, 'owned');
  const port = await reservePort();
  let runnerPid: number | undefined;
  let displayPid: number | undefined;
  let displayNumber: number | undefined;

  try {
    const program = withOwnedDirectory(
      owned,
      Effect.scoped(
        Effect.gen(function* () {
          const display = yield* displayResource(
            false,
            { ...process.env },
            path.join(owned, 'xvfb.log'),
          );
          displayPid = display.process?.pid;
          displayNumber = display.displayNumber;
          yield* runCommand({
            command: process.execPath,
            args: [
              '-e',
              "require('node:net').createServer().listen(Number(process.env.TEST_PORT), '127.0.0.1'); setInterval(() => {}, 1000)",
            ],
            cwd: process.cwd(),
            env: { ...process.env, TEST_PORT: String(port) },
            timeoutMs: 10_000,
            logPath: path.join(owned, 'runner.log'),
            onSpawn: (pid) => {
              runnerPid = pid;
            },
          });
        }),
      ),
    );
    const fiber = Effect.runFork(program);
    await waitUntil(
      async () => Boolean(runnerPid && displayNumber !== undefined && (await portIsOpen(port))),
      5000,
    );
    expect(runnerPid).toBeDefined();
    expect(displayPid).toBeDefined();
    expect(displayNumber).toBeDefined();
    expect(await portIsOpen(port)).toBe(true);

    await Effect.runPromise(Fiber.interrupt(fiber));
    await expect(access(owned)).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(
      Effect.runPromise(
        verifyTeardownTargets({
          displayNumber,
          port,
          processGroupPids: [runnerPid, displayPid].filter(
            (pid): pid is number => pid !== undefined,
          ),
        }),
      ),
    ).resolves.toEqual({ portClosed: true });
  } finally {
    await rm(parent, { force: true, recursive: true });
  }
});
