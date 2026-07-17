import { browser, $ } from '@wdio/globals';

export async function mountNativeApp(): Promise<void> {
  await browser.waitUntil(() => browser.execute(() => window.__JELLYPILOT_E2E__?.ready === true), {
    timeout: 30_000,
    timeoutMsg: 'The controlled Tauri bridge did not become ready before mount.',
  });
  await browser.execute(() => window.__JELLYPILOT_E2E__?.mount?.());
  const heading = await $('aria/JellyPilot');
  await heading.waitForDisplayed({ timeout: 30_000 });
}
