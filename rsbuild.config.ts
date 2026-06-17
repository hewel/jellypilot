import { defineConfig } from '@rsbuild/core';
import { pluginBabel } from '@rsbuild/plugin-babel';
import { pluginSolid } from '@rsbuild/plugin-solid';
import { pluginTailwindcss } from '@rsbuild/plugin-tailwindcss';
import { tanstackRouter } from '@tanstack/router-plugin/rspack';

// Docs: https://rsbuild.rs/config/
export default defineConfig({
  plugins: [
    pluginBabel({
      include: /\.(?:jsx|tsx)$/,
    }),
    pluginSolid(),
    pluginTailwindcss(),
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
          target: 'solid',
          autoCodeSplitting: true,
        }),
      ],
    },
  },
});
