type TestMock = (args?: Record<string, unknown>, options?: unknown) => unknown;

export * from '../../../node_modules/@tauri-apps/api/core.js';

declare global {
  interface Window {
    __JELLYPILOT_TEST_INVOKE__?: typeof invoke;
    __wdio_mocks__?: Record<string, TestMock>;
  }
}

export function invoke<T>(
  command: string,
  args: Record<string, unknown> = {},
  options?: unknown,
): Promise<T> {
  const mock = window.__wdio_mocks__?.[command];
  if (!mock) {
    return Promise.reject(new Error(`Rejected undeclared test IPC command: ${command}`));
  }

  return Promise.resolve(mock(args, options) as T);
}

window.__JELLYPILOT_TEST_INVOKE__ = invoke;
