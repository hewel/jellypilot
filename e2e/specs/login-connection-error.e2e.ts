import { $, $$, browser, expect } from '@wdio/globals';

import { mountNativeApp } from '../support/native-app';

describe('Login connection failure', () => {
  it('renders the typed network failure without remembering credentials', async () => {
    await mountNativeApp();

    const passwordTab = await $('aria/Password');
    const serverHost = await $('aria/Jellyfin host');
    const username = await $('aria/Username');
    await passwordTab.click();
    await serverHost.setValue('media.invalid');
    await username.setValue('e2e-user');
    const passwordCandidates = await $$('aria/Password').getElements();
    let passwordInput: WebdriverIO.Element | undefined;
    let candidateIndex = 0;
    while (candidateIndex < passwordCandidates.length) {
      const candidate = passwordCandidates[candidateIndex];
      if (candidate && (await candidate.getComputedRole()) === 'textbox') {
        passwordInput = candidate;
        break;
      }
      candidateIndex += 1;
    }
    if (!passwordInput) throw new Error('Could not find the password textbox by accessible name.');
    await passwordInput.setValue('not-a-secret');

    const remember = await $('aria/Remember Server URL and username');
    expect(await remember.isSelected()).toBe(false);

    const connect = await $('button=Connect');
    await connect.click();

    const alert = await $('[role="alert"]');
    await alert.waitForDisplayed();
    expect(await alert.getText()).toContain('Connection needs attention');
    expect(await alert.getText()).toContain('E2E fixture: server unreachable');

    await browser.waitUntil(() => connect.isEnabled(), {
      timeoutMsg: 'Connect remained disabled after the fixture failure.',
    });
    expect(await browser.execute(() => window.location.pathname)).toBe('/login');
    expect(
      await browser.execute(() => window.__JELLYPILOT_E2E__?.callCount('server_connect')),
    ).toBe(1);
    expect(
      await browser.execute(() => window.__JELLYPILOT_E2E__?.hasExpectedServerConnectCall()),
    ).toBe(true);
    expect(
      await browser.execute(() => [
        localStorage.getItem('jellypilot_saved_credentials'),
        localStorage.getItem('jmsr_saved_credentials'),
      ]),
    ).toEqual([null, null]);
  });
});
