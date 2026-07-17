import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@fontsource-variable/inter';
import '@fontsource-variable/space-grotesk';
import { init as initializeWdio, waitForInit } from '@wdio/tauri-plugin';

import { invoke as realInvoke } from '../../node_modules/@tauri-apps/api/core.js';

import '../../src/index.css';
import { mountApplication } from '../../src/mountApplication';
import {
  createControlledInvoke,
  fixtureCallCount,
  fixtureSummary,
  hasExpectedServerConnectCall,
  installFixture,
  installStartupFixtures,
} from './fixture-registry';

declare global {
  interface Window {
    __JELLYPILOT_E2E__?: {
      readonly ready: true;
      callCount: typeof fixtureCallCount;
      fixtureSummary: typeof fixtureSummary;
      hasExpectedServerConnectCall: typeof hasExpectedServerConnectCall;
      installFixture: typeof installFixture;
      installStartupFixtures: typeof installStartupFixtures;
      invokeForTest: ReturnType<typeof createControlledInvoke>;
      mount?: () => void;
    };
  }
}

attachDevtoolsOverlay();

await initializeWdio();
await waitForInit();

const invokeForTest = createControlledInvoke(realInvoke);

const controller: NonNullable<Window['__JELLYPILOT_E2E__']> = {
  ready: true,
  callCount: fixtureCallCount,
  fixtureSummary,
  hasExpectedServerConnectCall,
  installFixture,
  installStartupFixtures,
  invokeForTest,
  mount: () => {
    controller.mount = undefined;
    mountApplication();
  },
};

window.__JELLYPILOT_E2E__ = controller;
