import { createHash } from 'node:crypto';
import { access, lstat, readFile, readdir, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { Effect, Schema } from 'effect';

import { BINARY_PATH, MANIFEST_PATH, REPO_ROOT } from '../constants';
import { E2eBuildError, E2eStaleBuildError } from './errors';
import type { ToolVersions } from './preflight';

const INPUT_PATHS = [
  'src',
  'styled-system',
  'e2e/app',
  'src-tauri/src',
  'src-tauri/media-server-api',
  'src-tauri/capabilities',
  'src-tauri/icons',
  'src-tauri/build.rs',
  'src-tauri/Cargo.toml',
  'src-tauri/Cargo.lock',
  'src-tauri/tauri.conf.json',
  'src-tauri/tauri.webdriver.conf.json',
  'package.json',
  'bun.lock',
  'index.html',
  'rsbuild.config.ts',
  'panda.config.ts',
  'postcss.config.mjs',
  'tsconfig.json',
] as const;

export interface BuildManifest {
  readonly schemaVersion: 1;
  readonly appFingerprint: string;
  readonly binaryHash: string;
  readonly builtAt: string;
  readonly toolVersions: ToolVersions;
  readonly variables: {
    readonly PUBLIC_WEBDRIVER: '1';
    readonly CARGO_TARGET_DIR: string;
    readonly TAURI_CONFIG: 'src-tauri/tauri.webdriver.conf.json';
  };
}

const BuildManifestSchema = Schema.Struct({
  schemaVersion: Schema.Literal(1),
  appFingerprint: Schema.String,
  binaryHash: Schema.String,
  builtAt: Schema.String,
  toolVersions: Schema.Struct({ bun: Schema.String, node: Schema.String, rust: Schema.String }),
  variables: Schema.Struct({
    PUBLIC_WEBDRIVER: Schema.Literal('1'),
    CARGO_TARGET_DIR: Schema.String,
    TAURI_CONFIG: Schema.Literal('src-tauri/tauri.webdriver.conf.json'),
  }),
});

function isMissingPathError(cause: unknown): boolean {
  return (
    typeof cause === 'object' &&
    cause !== null &&
    'code' in cause &&
    (cause.code === 'ENOENT' || cause.code === 'ENOTDIR')
  );
}

async function listFiles(target: string): Promise<string[]> {
  const stat = await lstat(target);
  if (stat.isFile()) return [target];
  if (!stat.isDirectory()) return [];

  const entries = await readdir(target, { withFileTypes: true });
  const nested = await Promise.all(
    entries
      .filter((entry) => entry.name !== 'target')
      .map((entry) => listFiles(path.join(target, entry.name))),
  );
  return nested.flat();
}

async function hashFile(filePath: string): Promise<string> {
  const contents = await readFile(filePath);
  return createHash('sha256').update(contents).digest('hex');
}

export async function computeAppFingerprintAt(root: string): Promise<string> {
  const inputFiles = await Promise.all(
    INPUT_PATHS.map(async (relativePath) => {
      const absolutePath = path.join(root, relativePath);
      try {
        return await listFiles(absolutePath);
      } catch (error) {
        if (isMissingPathError(error)) return [];
        throw error;
      }
    }),
  );
  const files = inputFiles.flat().toSorted();

  const hash = createHash('sha256');
  for (const filePath of files) {
    const relativePath = path.relative(root, filePath);
    hash.update(relativePath);
    hash.update('\0');
    hash.update(await readFile(filePath));
    hash.update('\0');
  }
  return hash.digest('hex');
}

export const computeAppFingerprint = Effect.fn('e2e.computeAppFingerprint')(() =>
  Effect.tryPromise({
    try: () => computeAppFingerprintAt(REPO_ROOT),
    catch: (cause) =>
      new E2eBuildError({ message: 'Could not fingerprint E2E application inputs.', cause }),
  }),
);

export const writeBuildManifest = Effect.fn('e2e.writeBuildManifest')(
  (appFingerprint: string, toolVersions: ToolVersions) =>
    Effect.tryPromise({
      try: async () => {
        const manifest: BuildManifest = {
          schemaVersion: 1,
          appFingerprint,
          binaryHash: await hashFile(BINARY_PATH),
          builtAt: new Date().toISOString(),
          toolVersions,
          variables: {
            PUBLIC_WEBDRIVER: '1',
            CARGO_TARGET_DIR: path.relative(REPO_ROOT, path.dirname(path.dirname(BINARY_PATH))),
            TAURI_CONFIG: 'src-tauri/tauri.webdriver.conf.json',
          },
        };
        const temporaryPath = `${MANIFEST_PATH}.${process.pid}.tmp`;
        await writeFile(temporaryPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
        await rename(temporaryPath, MANIFEST_PATH);
        return manifest;
      },
      catch: (cause) =>
        new E2eBuildError({ message: 'Could not write the E2E build manifest.', cause }),
    }),
);

export const verifyBuildManifest = Effect.fn('e2e.verifyBuildManifest')(function* () {
  const manifestJson = yield* Effect.tryPromise({
    try: async () => {
      await access(BINARY_PATH);
      return JSON.parse(await readFile(MANIFEST_PATH, 'utf8')) as unknown;
    },
    catch: () =>
      new E2eStaleBuildError({
        message: 'Native E2E build is missing. Run `bun run build:e2e` first.',
      }),
  });
  const manifest = yield* Schema.decodeUnknownEffect(BuildManifestSchema)(manifestJson).pipe(
    Effect.mapError(
      () =>
        new E2eStaleBuildError({
          message: 'Native E2E manifest is invalid. Run `bun run build:e2e` again.',
        }),
    ),
  );

  const [appFingerprint, binaryHash] = yield* Effect.tryPromise({
    try: () => Promise.all([computeAppFingerprintAt(REPO_ROOT), hashFile(BINARY_PATH)]),
    catch: () =>
      new E2eStaleBuildError({
        message: 'Could not verify the native E2E build. Run `bun run build:e2e` again.',
      }),
  });

  if (manifest.schemaVersion !== 1 || manifest.appFingerprint !== appFingerprint) {
    return yield* new E2eStaleBuildError({
      message: 'Native E2E application inputs changed. Run `bun run build:e2e` again.',
    });
  }
  if (manifest.binaryHash !== binaryHash) {
    return yield* new E2eStaleBuildError({
      message: 'Native E2E binary does not match its manifest. Run `bun run build:e2e` again.',
    });
  }

  return manifest;
});
