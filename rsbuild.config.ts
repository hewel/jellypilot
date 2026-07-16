import path from 'node:path';

import { defineConfig } from '@rsbuild/core';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';
import { tanstackRouter } from '@tanstack/router-plugin/rspack';

const rootDir = import.meta.dirname;

// Docs: https://rsbuild.rs/config/
export default defineConfig({
  plugins: [
    pluginBabel({
      include: /\.(?:jsx|tsx)$/,
    }),
    pluginSolid(),
  ],
  source: {
    alias: {
      '@styled-system': path.join(rootDir, 'styled-system'),
    },
  },
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
