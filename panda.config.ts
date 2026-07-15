import { defineConfig } from '@pandacss/dev';

export default defineConfig({
  conditions: {
    extend: {
      selectOpen: '[data-scope="select"][data-part="indicator"][data-state="open"] &',
    },
  },
  prefix: { cssVar: 'panda-prototype' },
  globalCss: {
    '[data-panda-prototype]': {
      background: 'jellypilot.background',
      color: 'jellypilot.onSurface',
      fontFamily: 'var(--jellypilot-font-sans)',
      minHeight: '100dvh',
    },
    '[data-panda-prototype] *, [data-panda-prototype] *::before, [data-panda-prototype] *::after': {
      boxSizing: 'border-box',
    },
  },
  importMap: '@styled-system',
  include: ['./src/**/*.{js,jsx,ts,tsx}'],
  jsxFramework: 'solid',
  outdir: 'styled-system',
  preflight: false,
  theme: {
    extend: {
      breakpoints: {
        sm: '640px',
        lg: '1024px',
      },
      keyframes: {
        pandaPrototypePulse: {
          '0%, 100%': { opacity: '1', transform: 'scale(1)' },
          '50%': { opacity: '0.45', transform: 'scale(0.82)' },
        },
      },
      semanticTokens: {
        colors: {
          jellypilot: {
            background: { value: 'var(--jellypilot-color-background)' },
            error: { value: 'var(--jellypilot-color-error)' },
            onSurface: { value: 'var(--jellypilot-color-on-surface)' },
            onSurfaceVariant: { value: 'var(--jellypilot-color-on-surface-variant)' },
            outline: { value: 'var(--jellypilot-color-outline)' },
            outlineVariant: { value: 'var(--jellypilot-color-outline-variant)' },
            primary: { value: 'var(--jellypilot-color-primary)' },
            secondary: { value: 'var(--jellypilot-color-secondary)' },
            surface: { value: 'var(--jellypilot-color-surface)' },
            surfaceContainer: { value: 'var(--jellypilot-color-surface-container)' },
            surfaceContainerHigh: {
              value: 'var(--jellypilot-color-surface-container-high)',
            },
            surfaceContainerHighest: {
              value: 'var(--jellypilot-color-surface-container-highest)',
            },
            tertiary: { value: 'var(--jellypilot-color-tertiary)' },
          },
        },
      },
    },
  },
});
