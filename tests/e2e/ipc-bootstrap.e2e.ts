import { $, browser, expect } from '@wdio/globals';

declare global {
  interface Window {
    __JELLYPILOT_MOUNT__?: () => void;
    __JELLYPILOT_TEST_INVOKE__?: (
      command: string,
      args?: Record<string, unknown>,
    ) => Promise<unknown>;
    __wdio_original_core__?: {
      invoke?: (...args: unknown[]) => Promise<unknown>;
    };
    __wdio_mocks__?: Record<
      string,
      {
        mock?: {
          calls?: unknown[][];
        };
      }
    >;
  }
}

const startupCommands = ['config_get', 'server_is_connected', 'server_profiles_get'] as const;

describe('native IPC bootstrap', () => {
  it('mocks every startup command before mounting the application', async () => {
    await browser.waitUntil(
      () => browser.execute(() => typeof window.__wdio_original_core__?.invoke === 'function'),
      {
        timeout: 30_000,
        timeoutMsg: 'The WebDriver IPC bridge never became ready.',
      },
    );

    const configGet = await browser.tauri.mock('config_get');
    await configGet.mockReturnValue({ themePreference: 'dark' });

    const serverIsConnected = await browser.tauri.mock('server_is_connected');
    await serverIsConnected.mockReturnValue(false);

    const serverProfilesGet = await browser.tauri.mock('server_profiles_get');
    await serverProfilesGet.mockReturnValue({ activeProfileKey: null, profiles: [] });

    await browser.waitUntil(
      () => browser.tauri.execute(() => typeof window.__JELLYPILOT_MOUNT__ === 'function'),
      {
        timeout: 30_000,
        timeoutMsg: 'The WebDriver test bootstrap never exposed the mount callback.',
      },
    );

    await browser.execute(() => window.__JELLYPILOT_MOUNT__?.());

    const host = await $('input[name="host"]');
    await host.waitForDisplayed();
    await expect(host).toBeDisplayed();

    await browser.waitUntil(
      () =>
        browser.execute(
          (commands: string[]) =>
            commands.every(
              (command) => window.__wdio_mocks__?.[command]?.mock?.calls?.length === 1,
            ),
          [...startupCommands],
        ),
      {
        timeout: 10_000,
        timeoutMsg: 'The application did not invoke every startup fixture once.',
      },
    );

    const startupCalls = await browser.execute(
      (commands: string[]) =>
        Object.fromEntries(
          commands.map((command) => [command, window.__wdio_mocks__?.[command]?.mock?.calls ?? []]),
        ),
      [...startupCommands],
    );
    expect(startupCalls.config_get).toHaveLength(1);
    expect(startupCalls.server_is_connected).toHaveLength(1);
    expect(startupCalls.server_profiles_get).toHaveLength(1);

    const unknownCommandError = await browser.execute(async () => {
      try {
        await window.__JELLYPILOT_TEST_INVOKE__?.('__undeclared_test_command__');
        return null;
      } catch (error) {
        return error instanceof Error ? error.message : String(error);
      }
    });

    expect(unknownCommandError).toContain(
      'Rejected undeclared test IPC command: __undeclared_test_command__',
    );
  });
});
