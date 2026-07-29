import { browser, expect } from '@wdio/globals';

import type { AppLocalServices } from '../../src/bindings';

describe('Local image proxy service', () => {
  it('starts a reachable ephemeral listener and exposes it through typed IPC', async () => {
    await browser.waitUntil(
      () => browser.execute(() => window.__JELLYPILOT_E2E__?.ready === true),
      {
        timeout: 30_000,
        timeoutMsg: 'The controlled Tauri bridge did not become ready.',
      },
    );

    const services = await browser.execute(() =>
      window.__JELLYPILOT_E2E__?.invokeForTest<AppLocalServices>('app_local_services'),
    );
    expect(services?.imageProxyBase).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);

    const response = await browser.execute(async (base) => {
      if (!base) throw new Error('Image proxy base URL is unavailable.');
      const result = await fetch(`${base}/image/invalid-token`);
      return result.status;
    }, services?.imageProxyBase);

    expect(response).toBe(400);
    expect(
      await browser.execute(() => window.__JELLYPILOT_E2E__?.callCount('app_local_services')),
    ).toBe(1);
  });
});
