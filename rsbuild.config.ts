import path from 'node:path';

import { defineConfig } from '@rsbuild/core';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';
import { tanstackRouter } from '@tanstack/router-plugin/rspack';

const rootDir = import.meta.dirname;
const webdriverBuild = process.env.PUBLIC_WEBDRIVER === '1';

// Docs: https://rsbuild.rs/config/
export default defineConfig({
  source: {
    entry: webdriverBuild
      ? {
          index: './e2e/app/index.tsx',
        }
      : undefined,
  },
  resolve: webdriverBuild
    ? {
        alias: {
          '@tauri-apps/api/core$': path.join(rootDir, 'e2e/app/tauri-core.ts'),
          '@styled-system': path.join(rootDir, 'styled-system'),
        },
      }
    : undefined,
  output: webdriverBuild
    ? {
        distPath: {
          root: '.artifacts/e2e/build/frontend',
        },
      }
    : undefined,
  plugins: [
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
          routeFileIgnorePattern: '\\.styles\\.ts$',
          target: 'solid',
        }),
      ],
    },
  },
});
