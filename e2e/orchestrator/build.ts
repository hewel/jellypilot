import path from 'node:path';

import { Effect } from 'effect';

import { BUILD_ROOT, BUILD_TIMEOUT_MS, E2E_ROOT, REPO_ROOT, TARGET_ROOT } from '../constants';
import { E2eBuildError } from '../support/errors';
import {
  computeAppFingerprint,
  verifyBuildManifest,
  writeBuildManifest,
} from '../support/fingerprint';
import { ensureOwnedDirectory, removeOwnedDirectory } from '../support/ownership';
import { preflight } from '../support/preflight';
import { runCommand } from '../support/process';

export const buildE2e = Effect.fn('e2e.build')(function* (baseEnv: NodeJS.ProcessEnv) {
  const toolVersions = yield* preflight();
  yield* ensureOwnedDirectory(E2E_ROOT);
  yield* removeOwnedDirectory(BUILD_ROOT);
  yield* ensureOwnedDirectory(BUILD_ROOT);

  const beforeFingerprint = yield* computeAppFingerprint();
  const env = {
    ...baseEnv,
    BUN_TMPDIR: '/tmp',
    CARGO_TARGET_DIR: TARGET_ROOT,
    PUBLIC_WEBDRIVER: '1',
  };

  yield* runCommand({
    command: 'bun',
    args: [
      'tauri',
      'build',
      '--debug',
      '--no-bundle',
      '--features',
      'webdriver',
      '--config',
      'src-tauri/tauri.webdriver.conf.json',
    ],
    cwd: REPO_ROOT,
    env,
    timeoutMs: BUILD_TIMEOUT_MS,
    logPath: path.join(BUILD_ROOT, 'build.log'),
  });

  const afterFingerprint = yield* computeAppFingerprint();
  if (beforeFingerprint !== afterFingerprint) {
    return yield* new E2eBuildError({
      message: 'E2E application inputs changed during the build; rerun `bun run build:e2e`.',
    });
  }

  return yield* writeBuildManifest(afterFingerprint, toolVersions);
});

export const checkExistingBuild = verifyBuildManifest;
