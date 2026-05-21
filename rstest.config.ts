import { defineConfig } from '@rstest/core';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';

// Docs: https://rstest.rs/config/
export default defineConfig({
  plugins: [
    pluginBabel({
      include: /\.(?:jsx|tsx)$/,
    }),
    pluginSolid(),
  ],
  testEnvironment: 'jsdom',
  setupFiles: ['./rstest.setup.ts'],
  resolve: {
    alias: {
      'solid-js$': 'solid-js/dist/solid.js',
      'solid-js/web': 'solid-js/web/dist/web.js',
    },
  },
});
