import { defineConfig } from '@pandacss/dev';

import {
  appearanceSemanticColors,
  braunFonts,
  braunRadii,
  breakpoints,
  durations,
  easings,
  fonts,
  fontSizes,
  fontWeights,
  letterSpacings,
  lineHeights,
  radii,
  rawColors,
  semanticColorRefs,
  shadows,
  spacing,
  zIndex,
  type SemanticColorRole,
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

const modeAwareSemanticColors = (
  darkValues: Record<SemanticColorRole, string>,
  lightValues: Record<SemanticColorRole, string>,
) =>
  Object.fromEntries(
    (Object.keys(darkValues) as SemanticColorRole[]).map((role) => [
      role,
      {
        value: {
          base: darkValues[role],
          _light: lightValues[role],
        },
      },
    ]),
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
      light: '[data-color-mode=light] &',
      dark: '[data-color-mode=dark] &',
    },
  },
  staticCss: {
    themes: ['control-room', 'braun'],
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
      fontFamily: 'body',
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
  themes: {
    'control-room': {
      semanticTokens: {
        colors: modeAwareSemanticColors(
          appearanceSemanticColors['control-room'].dark,
          appearanceSemanticColors['control-room'].light,
        ),
        fonts: tokenEntries(fonts),
        radii: tokenEntries(radii),
        durations: tokenEntries(durations),
      },
    },
    braun: {
      tokens: {
        fonts: tokenEntries(braunFonts),
        radii: tokenEntries(braunRadii),
      },
      semanticTokens: {
        colors: modeAwareSemanticColors(
          appearanceSemanticColors.braun.dark,
          appearanceSemanticColors.braun.light,
        ),
        fonts: tokenEntries(braunFonts),
        radii: tokenEntries(braunRadii),
        durations: tokenEntries(durations),
      },
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
      letterSpacings: tokenEntries(letterSpacings),
      radii: tokenEntries(radii),
      shadows: tokenEntries(shadows),
      zIndex: tokenEntries(zIndex),
      durations: tokenEntries(durations),
      easings: tokenEntries(easings),
    },
    semanticTokens: {
      colors: {
        ...semanticColorTokens,
        success: { value: '{colors.teal.400}' },
        onSuccess: { value: '{colors.teal.1000}' },
        successContainer: { value: '{colors.teal.900}' },
        onSuccessContainer: { value: '{colors.teal.50}' },
        successIndicator: { value: '{colors.teal.400}' },
        info: { value: '{colors.indigo.300}' },
        onInfo: { value: '{colors.indigo.1000}' },
        infoContainer: { value: '{colors.indigo.900}' },
        onInfoContainer: { value: '{colors.indigo.50}' },
        infoIndicator: { value: '{colors.indigo.300}' },
        neutral: { value: '{colors.neutral.300}' },
        onNeutral: { value: '{colors.neutral.975}' },
        neutralContainer: { value: '{colors.neutral.800}' },
        onNeutralContainer: { value: '{colors.neutral.50}' },
        neutralIndicator: { value: '{colors.neutral.300}' },
        warningIndicator: { value: '{colors.amber.400}' },
        errorIndicator: { value: '{colors.red.400}' },
        focusRing: { value: '{colors.indigo.300}' },
        artworkOutline: { value: '{colors.neutral.0}' },
        artworkShadow: { value: '{colors.neutral.1000}' },
        materialSurfaceRaised: { value: '{colors.neutral.900}' },
        materialSurfaceRecessed: { value: '{colors.neutral.950}' },
        materialSurfaceAcrylic: { value: '{colors.neutral.900}' },
        materialSurfaceGlass: { value: '{colors.neutral.850}' },
        materialSurfaceKey: { value: '{colors.neutral.850}' },
        materialSurfaceKeyHover: { value: '{colors.neutral.800}' },
        materialSurfacePressed: { value: '{colors.neutral.750}' },
        materialEdgeSubtle: { value: '{colors.neutral.700}' },
        materialEdgeNormal: { value: '{colors.neutral.500}' },
        materialEdgeStrong: { value: '{colors.neutral.300}' },
        materialEdgeSpecular: { value: '{colors.neutral.50}' },
        materialDepthAmbient: { value: '{colors.neutral.1000}' },
        materialDepthRaised: { value: '{colors.neutral.1000}' },
        materialDepthRecessed: { value: '{colors.neutral.1000}' },
        materialDepthOverlay: { value: '{colors.neutral.1000}' },
        materialDepthKeycap: { value: '{colors.neutral.1000}' },
        materialDepthPressed: { value: '{colors.neutral.1000}' },
        materialDepthIndicator: { value: '{colors.teal.400}' },
      },
      fonts: {
        body: { value: "'Inter Variable', ui-sans-serif, system-ui, sans-serif" },
        display: {
          value: "'Space Grotesk Variable', 'Inter Variable', ui-sans-serif, system-ui, sans-serif",
        },
        readout: {
          value: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        },
      },
      radii: {
        panel: { value: '{radii.2xl}' },
        control: { value: '{radii.xl}' },
        overlay: { value: '{radii.3xl}' },
        artwork: { value: '{radii.xl}' },
        indicator: { value: '{radii.full}' },
      },
      durations: {
        interaction: { value: '{durations.150}' },
        overlay: { value: '{durations.200}' },
        appearance: { value: '{durations.200}' },
        mechanical: { value: '{durations.100}' },
      },
    },
  },
});
