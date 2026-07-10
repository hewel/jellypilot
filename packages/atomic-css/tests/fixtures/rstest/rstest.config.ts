import { pluginAtomic } from '@jellypilot/atomic-css/rsbuild';
import { defineConfig } from '@rstest/core';

export default defineConfig({
  plugins: [pluginAtomic()],
  testEnvironment: 'node',
  include: ['styles.test.ts'],
});
