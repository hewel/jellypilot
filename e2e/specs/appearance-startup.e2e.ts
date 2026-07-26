import { browser, expect } from '@wdio/globals';

import {
  APPEARANCE_STARTUP_ENV,
  appearanceStartupCaseById,
  isAppearanceStartupCaseId,
} from '../support/appearance-startup';
import { mountNativeApp } from '../support/native-app';

function selectedAppearanceCase() {
  const raw = process.env[APPEARANCE_STARTUP_ENV];
  if (!raw || !isAppearanceStartupCaseId(raw)) {
    throw new Error(
      `Appearance startup E2E requires ${APPEARANCE_STARTUP_ENV} to name one case; received ${String(raw)}.`,
    );
  }
  return appearanceStartupCaseById(raw);
}

describe('appearance startup first paint', () => {
  it('boots the selected persisted Appearance with matching native and DOM state', async () => {
    const expected = selectedAppearanceCase();

    await browser.waitUntil(
      () => browser.execute(() => window.__JELLYPILOT_E2E__?.ready === true),
      {
        timeout: 30_000,
        timeoutMsg: 'The controlled Tauri bridge did not become ready before appearance setup.',
      },
    );

    await browser.execute(() => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller) throw new Error('Missing E2E controller');
      controller.installFixture('appearance_get', { kind: 'real' });
      controller.installFixture('appearance_ready', { kind: 'real' });
      controller.installFixture('plugin:window|is_visible', { kind: 'real' });
      controller.installFixture('plugin:window|theme', { kind: 'real' });
    });

    const hiddenBeforeMount = await browser.execute(async () => {
      const controller = window.__JELLYPILOT_E2E__;
      if (!controller) throw new Error('Missing E2E controller');
      return controller.invokeForTest<boolean>('plugin:window|is_visible', { label: 'main' });
    });
    expect(hiddenBeforeMount).toBe(false);

    await mountNativeApp();

    await browser.waitUntil(
      async () => {
        const visible = await browser.execute(async () => {
          const controller = window.__JELLYPILOT_E2E__;
          if (!controller) throw new Error('Missing E2E controller');
          return controller.invokeForTest<boolean>('plugin:window|is_visible', { label: 'main' });
        });
        return visible === true;
      },
      {
        timeout: 30_000,
        timeoutMsg: 'Main window did not become visible after appearance readiness.',
      },
    );

    const snapshot = await browser.execute(
      async (expectedAppearance, expectedCanvas) => {
        const root = document.documentElement;
        const body = document.body;
        const controller = window.__JELLYPILOT_E2E__;
        if (!controller) throw new Error('Missing E2E controller');

        return {
          pandaTheme: root.dataset.pandaTheme ?? null,
          colorMode: root.dataset.colorMode ?? null,
          colorScheme: root.style.colorScheme || getComputedStyle(root).colorScheme,
          bodyBackground: getComputedStyle(body).backgroundColor,
          readyCalls: controller.callCount('appearance_ready'),
          getCalls: controller.callCount('appearance_get'),
          hasExpectedReadyCall: controller.hasExpectedAppearanceReadyCall(
            expectedAppearance,
            expectedCanvas,
          ),
          appearance: await controller.invokeForTest<{
            designTheme: string;
            colorMode: string;
          }>('appearance_get'),
          nativeTheme: await controller.invokeForTest<string | null>('plugin:window|theme', {
            label: 'main',
          }),
          visible: await controller.invokeForTest<boolean>('plugin:window|is_visible', {
            label: 'main',
          }),
        };
      },
      expected.appearance,
      expected.canvas,
    );

    expect(snapshot.visible).toBe(true);
    expect(snapshot.nativeTheme).toBe(expected.nativeTheme);
    expect(snapshot.appearance).toEqual(expected.appearance);
    expect(snapshot.pandaTheme).toBe(expected.pandaTheme);
    expect(snapshot.colorMode).toBe(expected.colorMode);
    expect(snapshot.colorScheme).toContain(expected.colorMode);
    expect(snapshot.bodyBackground.replaceAll(/\s+/g, '')).toBe(
      expected.canvasCss.replaceAll(/\s+/g, ''),
    );
    expect(snapshot.getCalls).toBeGreaterThanOrEqual(1);
    expect(snapshot.readyCalls).toBe(1);
    expect(snapshot.hasExpectedReadyCall).toBe(true);
  });
});
