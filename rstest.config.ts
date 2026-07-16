import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';
import { defineConfig } from '@rstest/core';
import { VanillaExtractPlugin } from '@vanilla-extract/webpack-plugin';

// Docs: https://rstest.rs/config/
export default defineConfig({
  plugins: [
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
      '@styled-system': './styled-system',
    },
  },
  setupFiles: ['./rstest.setup.ts'],
  testEnvironment: 'jsdom',
  tools: {
    rspack: {
      plugins: [new VanillaExtractPlugin()],
    },
  },
});
