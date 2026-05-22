import { expect } from '@rstest/core';
import * as jestDomMatchers from '@testing-library/jest-dom/matchers';

expect.extend(jestDomMatchers);

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

Object.assign(window, {
  ResizeObserver: TestResizeObserver,
});

Object.assign(window, {
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: () => undefined,
  },
  __TAURI_INTERNALS__: {
    transformCallback: () => 1,
    unregisterCallback: () => undefined,
    invoke: async (cmd: string) => {
      if (cmd === 'plugin:app|version') {
        return 'test';
      }
      if (cmd === 'plugin:event|listen') {
        return 1;
      }
      return null;
    },
  },
});
