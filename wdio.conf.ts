import { resolve } from 'node:path';

const root = import.meta.dirname;
const binary = process.platform === 'win32' ? 'jellypilot.exe' : 'jellypilot';
const appBinaryPath = resolve(root, 'src-tauri', 'target', 'debug', binary);

export const config = {
  runner: 'local',
  specs: ['./tests/e2e/**/*.e2e.ts'],
  maxInstances: 1,
  logLevel: 'warn',
  waitforTimeout: 10_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 1,
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 90_000,
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
        startTimeout: 90_000,
      },
    ],
  ],
};
