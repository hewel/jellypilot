import { pluginAtomic } from '@jellypilot/atomic-css/rsbuild';
import { defineConfig } from '@rsbuild/core';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';

export default defineConfig({
  plugins: [pluginAtomic(), pluginBabel({ include: /\.(?:jsx|tsx)$/ }), pluginSolid()],
  source: {
    entry: {
      index: './src/index.tsx',
    },
  },
  output: {
    distPath: { root: 'dist' },
  },
  server: {
    port: 3100,
  },
});
