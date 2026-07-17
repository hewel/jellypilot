import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { browser } from '@wdio/globals';

const appBinaryPath = process.env.JELLYPILOT_E2E_BINARY;
const logDir = process.env.JELLYPILOT_E2E_LOG_DIR;
const embeddedPort = Number(process.env.JELLYPILOT_E2E_PORT);

if (!appBinaryPath || !logDir || !Number.isInteger(embeddedPort)) {
  throw new Error('The JellyPilot E2E orchestrator did not provide binary, log, and port inputs.');
}

export const config = {
  runner: 'local',
  maxInstances: 1,
  logLevel: 'warn',
  waitforTimeout: 15_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 0,
  specFileRetries: 0,
  framework: 'mocha',
  reporters: [['spec', { addConsoleLogs: true }]],
  mochaOpts: {
    ui: 'bdd',
    timeout: 240_000,
  },
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application: appBinaryPath,
      },
    },
  ],
  services: [
    [
      '@wdio/tauri-service',
      {
        appBinaryPath,
        captureBackendLogs: true,
        captureFrontendLogs: true,
        driverProvider: 'embedded',
        embeddedPort,
        logDir,
        startTimeout: 90_000,
      },
    ],
  ],
  afterTest: async (_test: unknown, _context: unknown, result: { error?: unknown }) => {
    if (!result.error) return;
    await mkdir(logDir, { recursive: true });
    await browser.saveScreenshot(path.join(logDir, 'screenshot.png')).catch(() => undefined);
    const summary = await browser
      .execute(() => window.__JELLYPILOT_E2E__?.fixtureSummary() ?? [])
      .catch(() => []);
    await writeFile(
      path.join(logDir, 'fixture-summary.json'),
      `${JSON.stringify(summary, null, 2)}\n`,
    );
  },
};
