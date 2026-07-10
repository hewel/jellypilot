import { resolve } from 'node:path';

import { pluginAtomic } from '@jellypilot/atomic-css/rsbuild';
import { defineConfig } from '@rsbuild/core';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';
import { tanstackRouter } from '@tanstack/router-plugin/rspack';

const webdriverBuild = process.env.PUBLIC_WEBDRIVER === '1';

// Docs: https://rsbuild.rs/config/
export default defineConfig({
  source: webdriverBuild
    ? {
        entry: {
          index: './src/index.webdriver.tsx',
        },
      }
    : undefined,
  resolve: webdriverBuild
    ? {
        alias: {
          '@tauri-apps/api/core': resolve(import.meta.dirname, 'tests/e2e/fixtures/tauri-core.ts'),
        },
      }
    : undefined,
  plugins: [
    pluginAtomic(),
    pluginBabel({
      include: /\.(?:jsx|tsx)$/,
    }),
    pluginSolid(),
  ],
  tools: {
    bundlerChain: (chain) => {
      chain.watchOptions({
        ignored: /src-tauri/,
      });
    },
    rspack: {
      plugins: [
        tanstackRouter({
          autoCodeSplitting: false,
          routeFileIgnorePattern: '\\.css\\.ts$',
          target: 'solid',
        }),
      ],
    },
  },
});
