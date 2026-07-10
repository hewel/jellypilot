import { pluginAtomic } from '@jellypilot/atomic-css/rsbuild';
import { defineConfig } from '@rsbuild/core';

export default defineConfig({
  plugins: [pluginAtomic()],
  source: {
    entry: {
      index: './src/index.ts',
    },
  },
  output: {
    target: 'web',
    distPath: { root: 'dist' },
    filenameHash: false,
  },
  tools: {
    htmlPlugin: false,
  },
});
