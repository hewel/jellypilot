import path from 'node:path';

export const REPO_ROOT = path.resolve(import.meta.dirname, '..');
export const E2E_ROOT = path.join(REPO_ROOT, '.artifacts/e2e');
export const BUILD_ROOT = path.join(E2E_ROOT, 'build');
export const FRONTEND_ROOT = path.join(BUILD_ROOT, 'frontend');
export const TARGET_ROOT = path.join(BUILD_ROOT, 'target');
export const BINARY_PATH = path.join(TARGET_ROOT, 'debug', 'jellypilot');
export const MANIFEST_PATH = path.join(BUILD_ROOT, 'manifest.json');
export const RUNS_ROOT = path.join(E2E_ROOT, 'runs');
export const FAILURES_ROOT = path.join(E2E_ROOT, 'failures');
export const ISOLATION_ROOT = path.join(E2E_ROOT, 'isolation');
export const OWNERSHIP_FILE = '.jellypilot-e2e-owned';
export const OWNERSHIP_MARKER = 'jellypilot-e2e-owned-v1';

export const BUILD_TIMEOUT_MS = 15 * 60_000;
export const SPEC_TIMEOUT_MS = 5 * 60_000;
export const SPEC_PROCESS_TIMEOUT_MS = SPEC_TIMEOUT_MS - 15_000;

export const E2E_SENTINELS = [
  '__JELLYPILOT_E2E__',
  'Rejected undeclared E2E IPC command',
  'JELLYPILOT_WEBDRIVER_REQUIRES_DEBUG_ASSERTIONS',
  'top.pigfun.jellypilot.webdriver',
  'tauri_plugin_wdio',
  'tauri-plugin-wdio',
] as const;
