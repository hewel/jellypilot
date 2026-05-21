import { expect } from '@rstest/core';
import * as jestDomMatchers from '@testing-library/jest-dom/matchers';

expect.extend(jestDomMatchers);

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke: async (cmd: string) => {
      if (cmd === 'plugin:app|version') {
        return 'test';
      }
      return null;
    },
  },
});
