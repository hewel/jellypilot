import { browser, expect } from '@wdio/globals';

import { mountNativeApp } from '../support/native-app';

describe('compiled Login smoke', () => {
  it('mounts after bridge readiness and crosses the real config_default IPC boundary', async () => {
    await mountNativeApp();

    const defaults = await browser.execute(async () =>
      window.__JELLYPILOT_E2E__?.invokeForTest('config_default'),
    );

    expect(defaults).toEqual({
      deviceName: 'JellyPilot',
      imageDiskCacheEnabled: true,
      introSkipperMode: 'automatic',
      keybindIntroSkip: 'g',
      keybindNext: 'Shift+>',
      keybindPrev: 'Shift+<',
      mpvArgs: [],
      mpvPath: null,
      preferredSubtitleLanguages: [],
      progressInterval: 5,
      startMinimized: false,
    });
  });
});
