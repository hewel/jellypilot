import { mkdir, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';

import { Effect, Schema } from 'effect';

import { BUILD_TIMEOUT_MS, E2E_ROOT, E2E_SENTINELS, ISOLATION_ROOT, REPO_ROOT } from '../constants';
import { E2eIsolationError } from '../support/errors';
import { ensureOwnedDirectory, removeOwnedDirectory } from '../support/ownership';
import { preflight } from '../support/preflight';
import { runCommand } from '../support/process';

const ProductionConfigSchema = Schema.Struct({
  app: Schema.Struct({
    withGlobalTauri: Schema.Boolean,
    security: Schema.Struct({ capabilities: Schema.Array(Schema.String) }),
  }),
});

async function scanPath(target: string, sentinels: readonly string[]): Promise<string[]> {
  const entries = await readdir(target, { withFileTypes: true });
  const matches: string[] = [];
  for (const entry of entries) {
    const entryPath = path.join(target, entry.name);
    if (entry.isDirectory()) {
      matches.push(...(await scanPath(entryPath, sentinels)));
      continue;
    }
    if (!entry.isFile()) continue;
    const contents = await readFile(entryPath);
    for (const sentinel of sentinels) {
      if (contents.includes(Buffer.from(sentinel))) matches.push(`${entryPath}: ${sentinel}`);
    }
  }
  return matches;
}

async function scanFile(target: string, sentinels: readonly string[]): Promise<string[]> {
  const contents = await readFile(target);
  return sentinels
    .filter((sentinel) => contents.includes(Buffer.from(sentinel)))
    .map((sentinel) => `${target}: ${sentinel}`);
}

const assertReleaseGuard = Effect.fn('e2e.assertReleaseGuard')(function* (env: NodeJS.ProcessEnv) {
  const result = yield* runCommand({
    command: 'cargo',
    args: [
      'check',
      '--manifest-path',
      'src-tauri/Cargo.toml',
      '--release',
      '--features',
      'webdriver',
    ],
    cwd: REPO_ROOT,
    env,
    timeoutMs: BUILD_TIMEOUT_MS,
    acceptNonZero: true,
  });
  if (result.exitCode === 0) {
    return yield* new E2eIsolationError({
      message: 'Optimized WebDriver build unexpectedly succeeded.',
    });
  }
  if (!result.output.includes('JELLYPILOT_WEBDRIVER_REQUIRES_DEBUG_ASSERTIONS')) {
    return yield* new E2eIsolationError({
      message: 'Optimized WebDriver build failed without the debug-only guard marker.',
    });
  }
});

export const checkProductionIsolation = Effect.fn('e2e.checkProductionIsolation')(function* (
  baseEnv: NodeJS.ProcessEnv,
) {
  yield* preflight();
  yield* ensureOwnedDirectory(E2E_ROOT);
  yield* removeOwnedDirectory(ISOLATION_ROOT);
  yield* ensureOwnedDirectory(ISOLATION_ROOT);

  const target = path.join(ISOLATION_ROOT, 'target');
  const xdgRoot = path.join(ISOLATION_ROOT, 'xdg');
  const env = {
    ...baseEnv,
    BUN_TMPDIR: '/tmp',
    CARGO_TARGET_DIR: target,
    TMPDIR: path.join(ISOLATION_ROOT, 'tmp'),
    XDG_CACHE_HOME: path.join(xdgRoot, 'cache'),
    XDG_CONFIG_HOME: path.join(xdgRoot, 'config'),
    XDG_DATA_HOME: path.join(xdgRoot, 'data'),
    XDG_RUNTIME_DIR: path.join(xdgRoot, 'runtime'),
    XDG_STATE_HOME: path.join(xdgRoot, 'state'),
  };
  yield* Effect.tryPromise({
    try: () =>
      Promise.all([
        mkdir(env.TMPDIR, { recursive: true }),
        mkdir(env.XDG_CACHE_HOME, { recursive: true }),
        mkdir(env.XDG_CONFIG_HOME, { recursive: true }),
        mkdir(env.XDG_DATA_HOME, { recursive: true }),
        mkdir(env.XDG_RUNTIME_DIR, { mode: 0o700, recursive: true }),
        mkdir(env.XDG_STATE_HOME, { recursive: true }),
      ]),
    catch: (cause) =>
      new E2eIsolationError({
        message: `Could not create the production-isolation sandbox: ${String(cause)}`,
      }),
  });

  yield* runCommand({
    command: 'bun',
    args: ['tauri', 'build', '--no-bundle'],
    cwd: REPO_ROOT,
    env,
    timeoutMs: BUILD_TIMEOUT_MS,
    logPath: path.join(ISOLATION_ROOT, 'build.log'),
  });

  const configJson = yield* Effect.tryPromise({
    try: async () =>
      JSON.parse(
        await readFile(path.join(REPO_ROOT, 'src-tauri/tauri.conf.json'), 'utf8'),
      ) as unknown,
    catch: (cause) => new E2eIsolationError({ message: String(cause) }),
  });
  const config = yield* Schema.decodeUnknownEffect(ProductionConfigSchema)(configJson).pipe(
    Effect.mapError(
      (cause) =>
        new E2eIsolationError({ message: `Production Tauri config is invalid: ${String(cause)}` }),
    ),
  );
  if (
    config.app?.withGlobalTauri !== false ||
    JSON.stringify(config.app?.security?.capabilities) !== JSON.stringify(['default'])
  ) {
    return yield* new E2eIsolationError({
      message:
        'Production Tauri config must select only the default capability and global API off.',
    });
  }

  const dependencyTree = yield* runCommand({
    command: 'cargo',
    args: ['tree', '--manifest-path', 'src-tauri/Cargo.toml', '--edges', 'normal'],
    cwd: REPO_ROOT,
    env,
    timeoutMs: BUILD_TIMEOUT_MS,
  });
  if (
    dependencyTree.output.includes('tauri-plugin-wdio ') ||
    dependencyTree.output.includes('tauri-plugin-wdio-webdriver ')
  ) {
    return yield* new E2eIsolationError({
      message: 'Production Cargo graph contains optional WebDriver plugins.',
    });
  }

  const frontendMatches = yield* Effect.tryPromise({
    try: () => scanPath(path.join(REPO_ROOT, 'dist'), E2E_SENTINELS),
    catch: (cause) =>
      new E2eIsolationError({
        message: `Could not scan the production frontend: ${String(cause)}`,
      }),
  });
  const binaryMatches = yield* Effect.tryPromise({
    try: () => scanFile(path.join(target, 'release', 'jellypilot'), E2E_SENTINELS),
    catch: (cause) =>
      new E2eIsolationError({ message: `Could not scan the production binary: ${String(cause)}` }),
  });
  const matches = [...frontendMatches, ...binaryMatches];
  if (matches.length > 0) {
    return yield* new E2eIsolationError({
      message: `Production output contains E2E markers:\n${matches.join('\n')}`,
    });
  }

  yield* assertReleaseGuard({
    ...env,
    CARGO_TARGET_DIR: path.join(ISOLATION_ROOT, 'guard-target'),
  });
});
