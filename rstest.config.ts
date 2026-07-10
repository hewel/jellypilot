import { pluginAtomic } from '@jellypilot/atomic-css/rsbuild';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';
import { defineConfig } from '@rstest/core';

// Docs: https://rstest.rs/config/
export default defineConfig({
  plugins: [
    pluginAtomic(),
    pluginBabel({
      include: /\.(?:jsx|tsx)$/,
    }),
    pluginSolid(),
  ],
  resolve: {
    alias: {
      'solid-js$': 'solid-js/dist/solid.js',
      'solid-js/store': 'solid-js/store/dist/store.js',
      'solid-js/web': 'solid-js/web/dist/web.js',
    },
  },
  setupFiles: ['./rstest.setup.ts'],
  testEnvironment: 'jsdom',
  // Package suites run under packages/*/rstest.config.ts with pluginAtomic.
  exclude: ['**/node_modules/**', '**/dist/**', 'packages/**'],
});
