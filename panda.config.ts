import { defineConfig } from '@pandacss/dev';

import {
  breakpoints,
  durations,
  easings,
  fonts,
  fontSizes,
  fontWeights,
  lineHeights,
  radii,
  rawColors,
  semanticColorRefs,
  shadows,
  spacing,
  zIndex,
} from './src/styles/theme-tokens';

const tokenEntries = <T extends Record<string, string>>(values: T) =>
  Object.fromEntries(Object.entries(values).map(([key, value]) => [key, { value }])) as {
    [K in keyof T]: { value: T[K] };
  };

const rawColorTokens = Object.fromEntries(
  Object.entries(rawColors).map(([palette, steps]) => [
    palette,
    Object.fromEntries(Object.entries(steps).map(([step, value]) => [step, { value }])),
  ]),
);

const semanticColorTokens = Object.fromEntries(
  Object.entries(semanticColorRefs).map(([role, ref]) => [role, { value: `{${ref}}` }]),
);

export default defineConfig({
  // Empty presets drop opinionated default tokens while keeping base utilities.
  presets: [],
  preflight: false,
  strictTokens: true,
  cssVarRoot: ':root',
  importMap: '@styled-system',
  include: ['./src/**/*.{js,jsx,ts,tsx}'],
  jsxFramework: 'solid',
  outdir: 'styled-system',
  conditions: {
    extend: {
      // Ark select open indicator (used by shared select styling later).
      selectOpen: '[data-scope="select"][data-part="indicator"][data-state="open"] &',
    },
  },
  globalCss: {
    '*, *::before, *::after': {
      boxSizing: 'border-box',
    },
    body: {
      margin: '0',
      minHeight: '100dvh',
      position: 'relative',
      color: 'onSurface',
      fontFamily: 'sans',
      WebkitFontSmoothing: 'antialiased',
      MozOsxFontSmoothing: 'grayscale',
      background: '{colors.background}',
    },
    'h1, h2, h3, h4, h5, h6': {
      textWrap: 'balance',
    },
    p: {
      textWrap: 'pretty',
      my: 0,
    },
    '@media (prefers-reduced-motion: reduce)': {
      '*, ::before, ::after': {
        animationDuration: '0.001ms !important',
        animationIterationCount: '1 !important',
        scrollBehavior: 'auto !important',
        transitionDuration: '0.001ms !important',
      },
    },
    '::-webkit-scrollbar': {
      width: '8px',
      height: '8px',
    },
    '::-webkit-scrollbar-track': {
      background: 'color-mix(in srgb, {colors.surfaceContainerLowest} 40%, transparent)',
    },
    '::-webkit-scrollbar-thumb': {
      background: 'color-mix(in srgb, {colors.outlineVariant} 80%, transparent)',
      backgroundClip: 'padding-box',
      border: '2px solid transparent',
      borderRadius: 'full',
    },
    '::-webkit-scrollbar-thumb:hover': {
      background: 'outline',
    },
  },
  theme: {
    breakpoints: { ...breakpoints },
    keyframes: {
      fadeIn: {
        from: { opacity: '0', transform: 'translateY(8px)' },
        to: { opacity: '1', transform: 'translateY(0)' },
      },
      'wave-bounce': {
        '0%': { transform: 'scaleY(0.25)' },
        '100%': { transform: 'scaleY(1)' },
      },
      pulse: {
        '50%': { opacity: '0.5' },
      },
      spin: {
        to: { transform: 'rotate(360deg)' },
      },
      ping: {
        '75%, 100%': { transform: 'scale(2)', opacity: '0' },
      },
      menuIn: {
        from: { opacity: '0', transform: 'translateY(-4px) scale(0.98)' },
        to: { opacity: '1', transform: 'translateY(0) scale(1)' },
      },
      sidebarLabelIn: {
        from: { opacity: '0', transform: 'translateX(-4px)' },
        to: { opacity: '1', transform: 'translateX(0)' },
      },
      iconSwapIn: {
        from: { opacity: '0', transform: 'scale(0.25)', filter: 'blur(4px)' },
        to: { opacity: '1', transform: 'scale(1)', filter: 'blur(0)' },
      },
      sidebarWipeExpand: {
        from: { transform: 'scaleX(0.28125)' },
        to: { transform: 'scaleX(1)' },
      },
      sidebarWipeCollapse: {
        from: { transform: 'scaleX(1)' },
        to: { transform: 'scaleX(0.28125)' },
      },
      sidebarGlideExpand: {
        from: { transform: 'translateX(-11.5rem)' },
        to: { transform: 'translateX(0)' },
      },
      sidebarGlideCollapse: {
        from: { transform: 'translateX(11.5rem)' },
        to: { transform: 'translateX(0)' },
      },
    },
    tokens: {
      colors: rawColorTokens,
      fonts: tokenEntries(fonts),
      spacing: tokenEntries(spacing),
      fontSizes: tokenEntries(fontSizes),
      sizes: tokenEntries({
        ...spacing,
        full: '100%',
        min: 'min-content',
        max: 'max-content',
        fit: 'fit-content',
      }),
      fontWeights: tokenEntries(fontWeights),
      lineHeights: tokenEntries(lineHeights),
      radii: tokenEntries(radii),
      shadows: tokenEntries(shadows),
      zIndex: tokenEntries(zIndex),
      durations: tokenEntries(durations),
      easings: tokenEntries(easings),
    },
    semanticTokens: {
      colors: semanticColorTokens,
    },
  },
});
