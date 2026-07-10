import { overrideToken, themeRef } from '@jellypilot/atomic-css/config';

export default {
  preset: 'preset-mini' as const,
  projectTheme: {
    tokens: {
      brand: themeRef('./theme.css.ts', 'brandSpace'),
    },
    conditions: {
      dark: "[data-theme='dark'] &",
      disabled: '&:disabled',
    },
  },
  overrides: [overrideToken('padding', 'brand', themeRef('./theme.css.ts', 'brandSpace'))],
};
