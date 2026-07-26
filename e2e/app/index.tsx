import { attachDevtoolsOverlay } from '@solid-devtools/overlay';
import '@fontsource-variable/archivo';
import '@fontsource-variable/inter';
import '@fontsource-variable/jetbrains-mono';
import '@fontsource-variable/space-grotesk';
import { init as initializeWdio, waitForInit } from '@wdio/tauri-plugin';

import { invoke as realInvoke } from '../../node_modules/@tauri-apps/api/core.js';

import '../../src/index.css';
import { mountApplication } from '../../src/mountApplication';
import {
  appearanceSetCalls,
  createControlledInvoke,
  fixtureCallCount,
  fixtureSummary,
  hasExpectedAppearanceReadyCall,
  hasExpectedAppearanceSetCall,
  hasExpectedLibraryPlayCall,
  hasExpectedReauthenticatePasswordCall,
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
      appearanceSetCalls: typeof appearanceSetCalls;
      hasExpectedAppearanceReadyCall: typeof hasExpectedAppearanceReadyCall;
      hasExpectedAppearanceSetCall: typeof hasExpectedAppearanceSetCall;
      hasExpectedLibraryPlayCall: typeof hasExpectedLibraryPlayCall;
      hasExpectedReauthenticatePasswordCall: typeof hasExpectedReauthenticatePasswordCall;
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
  appearanceSetCalls,
  hasExpectedAppearanceReadyCall,
  hasExpectedAppearanceSetCall,
  hasExpectedLibraryPlayCall,
  hasExpectedReauthenticatePasswordCall,
  hasExpectedServerConnectCall,
  installFixture,
  installStartupFixtures,
  invokeForTest,
  mount: () => {
    controller.mount = undefined;
    void mountApplication();
  },
};

window.__JELLYPILOT_E2E__ = controller;
