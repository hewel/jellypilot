#!/usr/bin/env bun
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const REQUIRED_WASM_PACK_VERSION = 'wasm-pack 0.15.0';
const cratePath = 'crates/jellypilot-core-wasm';
const repositoryRoot = resolve(import.meta.dirname, '..');

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function selectedProfile(): '--dev' | '--release' {
  const requestedProfile = process.argv[2];
  if (requestedProfile === '--dev' || requestedProfile === '--release') {
    return requestedProfile;
  }
  if (requestedProfile !== undefined) {
    fail(
      `Unknown library browse WASM profile: ${requestedProfile}\n` +
        'Usage: bun scripts/build-library-browse-wasm.ts [--dev|--release]',
    );
  }
  return process.env.PUBLIC_WEBDRIVER === '1' ? '--dev' : '--release';
}

const profile = selectedProfile();
const wasmPack = Bun.which('wasm-pack');
if (wasmPack === null) {
  fail(
    `${REQUIRED_WASM_PACK_VERSION} is required to build the library browse WASM package.\n` +
      'Install the pinned tool with `bun run task wasm install`.',
  );
}

const versionResult = Bun.spawnSync([wasmPack, '--version'], {
  cwd: repositoryRoot,
  stderr: 'pipe',
  stdout: 'pipe',
});
const installedVersion = versionResult.stdout.toString().trim();
if (versionResult.exitCode !== 0 || installedVersion !== REQUIRED_WASM_PACK_VERSION) {
  const found = installedVersion.length > 0 ? installedVersion : 'an unreadable version';
  fail(
    `Library browse WASM requires ${REQUIRED_WASM_PACK_VERSION}; found ${found}.\n` +
      'Install the pinned tool with `bun run task wasm install`.',
  );
}

const buildResult = Bun.spawnSync(
  [
    wasmPack,
    'build',
    cratePath,
    '--target',
    'web',
    '--out-dir',
    'pkg',
    '--out-name',
    'jellypilot_core_wasm',
    profile,
  ],
  {
    cwd: repositoryRoot,
    stderr: 'inherit',
    stdout: 'inherit',
  },
);

if (buildResult.exitCode !== 0) {
  fail(`Library browse WASM build failed with exit code ${buildResult.exitCode}.`);
}

const packagePath = resolve(repositoryRoot, cratePath, 'pkg');
const expectedOutputs = [
  resolve(packagePath, 'jellypilot_core_wasm.js'),
  resolve(packagePath, 'jellypilot_core_wasm_bg.wasm'),
];
if (!expectedOutputs.every(existsSync)) {
  fail(`Library browse WASM build did not create the expected web package in ${packagePath}.`);
}
