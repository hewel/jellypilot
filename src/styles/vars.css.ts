import {
  borderRadius as miniBorderRadius,
  breakpoints as miniBreakpoints,
  duration as miniDuration,
  fontSize as miniFontSize,
  fontWeight as miniFontWeight,
  lineHeight as miniLineHeight,
  spacing as miniSpacing,
} from '@unocss/preset-mini/theme';
import { createGlobalTheme, createGlobalThemeContract } from '@vanilla-extract/css';

const unoSpacing = miniSpacing as Record<string, string>;
const unoFontSize = miniFontSize as Record<string, readonly [string, string]>;
const unoFontWeight = miniFontWeight as Record<string, string>;
const unoLineHeight = miniLineHeight as Record<string, string>;
const unoBorderRadius = miniBorderRadius as Record<string, string>;
const unoDuration = miniDuration as Record<string, string>;
const unoBreakpoints = miniBreakpoints as Record<string, string>;

const spaceValue = {
  '0': unoSpacing.none,
  px: '1px',
  '0_5': '0.125rem',
  '1': '0.25rem',
  '1_5': '0.375rem',
  '2': '0.5rem',
  '2_5': '0.625rem',
  '3': '0.75rem',
  '3_5': '0.875rem',
  '4': '1rem',
  '5': '1.25rem',
  '6': '1.5rem',
  '7': '1.75rem',
  '8': '2rem',
  '9': '2.25rem',
  '10': '2.5rem',
  '11': '2.75rem',
  '12': '3rem',
  '14': '3.5rem',
  '16': '4rem',
  '20': '5rem',
  '24': '6rem',
  xs: unoSpacing.xs,
  sm: unoSpacing.sm,
  md: unoSpacing.DEFAULT,
  lg: unoSpacing.lg,
  xl: unoSpacing.xl,
  '2xl': unoSpacing['2xl'],
  '3xl': unoSpacing['3xl'],
  '4xl': unoSpacing['4xl'],
  '5xl': unoSpacing['5xl'],
  '6xl': unoSpacing['6xl'],
  '7xl': unoSpacing['7xl'],
  '8xl': unoSpacing['8xl'],
  '9xl': unoSpacing['9xl'],
};

const fontSizeValue = {
  '10': '0.625rem',
  '11': '0.6875rem',
  '12': unoFontSize.xs[0],
  '13': '0.8125rem',
  '14': unoFontSize.sm[0],
  '15': '0.9375rem',
  '16': unoFontSize.base[0],
  '18': unoFontSize.lg[0],
  '20': unoFontSize.xl[0],
  '22': '1.375rem',
  '24': unoFontSize['2xl'][0],
  '28': '1.75rem',
  '32': '2rem',
  '36': unoFontSize['4xl'][0],
  '45': '2.8125rem',
};

const lineHeightValue = {
  none: unoLineHeight.none,
  tight: unoLineHeight.tight,
  snug: unoLineHeight.snug,
  normal: unoLineHeight.normal,
  relaxed: unoLineHeight.relaxed,
  loose: unoLineHeight.loose,
  '14': '0.875rem',
  '16': '1rem',
  '20': '1.25rem',
  '22': '1.375rem',
  '24': '1.5rem',
  '28': '1.75rem',
  '32': '2rem',
  '40': '2.5rem',
  '44': '2.75rem',
  '52': '3.25rem',
};

const fontWeightValue = {
  normal: unoFontWeight.normal,
  medium: unoFontWeight.medium,
  semibold: unoFontWeight.semibold,
  bold: unoFontWeight.bold,
  extrabold: unoFontWeight.extrabold,
  black: unoFontWeight.black,
};

const borderRadiusValue = {
  none: unoBorderRadius.none,
  sm: unoBorderRadius.sm,
  md: unoBorderRadius.md,
  lg: unoBorderRadius.lg,
  xl: unoBorderRadius.xl,
  '2xl': unoBorderRadius['2xl'],
  '3xl': unoBorderRadius['3xl'],
  '4xl': '2rem',
  full: unoBorderRadius.full,
};

const shadowValue = {
  none: '0 0 rgb(0 0 0 / 0)',
  sm: '0 1px 2px 0 rgb(0 0 0 / 0.2)',
  md: '0 4px 8px -2px rgb(0 0 0 / 0.35)',
  lg: '0 10px 18px -6px rgb(0 0 0 / 0.45)',
  xl: '0 18px 30px -10px rgb(0 0 0 / 0.55)',
  '2xl': '0 25px 50px -12px rgb(0 0 0 / 0.65)',
  inner: 'inset 0 2px 4px 0 rgb(0 0 0 / 0.28)',
};

const zIndexValue = {
  auto: 'auto',
  '0': '0',
  '10': '10',
  '40': '40',
  '50': '50',
  '60': '60',
  '100': '100',
};

const durationValue = {
  none: unoDuration.none,
  '75': unoDuration['75'],
  '100': unoDuration['100'],
  '150': unoDuration['150'],
  '200': unoDuration['200'],
  '300': unoDuration['300'],
  '500': unoDuration['500'],
  '700': unoDuration['700'],
  '1000': unoDuration['1000'],
};

const easingValue = {
  standard: 'cubic-bezier(0.2, 0, 0, 1)',
  emphasized: 'cubic-bezier(0.16, 1, 0.3, 1)',
  linear: 'linear',
};

const breakpointValue = {
  sm: unoBreakpoints.sm,
  md: unoBreakpoints.md,
  lg: unoBreakpoints.lg,
  xl: unoBreakpoints.xl,
  '2xl': unoBreakpoints['2xl'],
};

export const breakpoints = breakpointValue;

const tokenContract = <Token extends string>(group: string, values: Record<Token, string>) =>
  Object.fromEntries(
    Object.keys(values).map((key) => [key, `--jellypilot-${group}-${key.replaceAll('_', '-')}`]),
  ) as Record<Token, string>;

/**
 * JellyPilot design-token source.
 *
 * This is the single owner of design token values. Tailwind utilities consume
 * these tokens via `@theme inline` aliases in `src/index.css`; component-local
 * vanilla-extract CSS consumes them directly through `vars.*`.
 */
export const vars = createGlobalThemeContract({
  color: {
    background: '--jellypilot-color-background',
    brandGlow: '--jellypilot-color-brand-glow',
    error: '--jellypilot-color-error',
    errorContainer: '--jellypilot-color-error-container',
    onBackground: '--jellypilot-color-on-background',
    onError: '--jellypilot-color-on-error',
    onErrorContainer: '--jellypilot-color-on-error-container',
    onPrimary: '--jellypilot-color-on-primary',
    onPrimaryContainer: '--jellypilot-color-on-primary-container',
    onSecondary: '--jellypilot-color-on-secondary',
    onSecondaryContainer: '--jellypilot-color-on-secondary-container',
    onSurface: '--jellypilot-color-on-surface',
    onSurfaceVariant: '--jellypilot-color-on-surface-variant',
    onTertiary: '--jellypilot-color-on-tertiary',
    onTertiaryContainer: '--jellypilot-color-on-tertiary-container',
    onWarning: '--jellypilot-color-on-warning',
    onWarningContainer: '--jellypilot-color-on-warning-container',
    outline: '--jellypilot-color-outline',
    outlineVariant: '--jellypilot-color-outline-variant',
    primary: '--jellypilot-color-primary',
    primaryContainer: '--jellypilot-color-primary-container',
    primaryGradientEnd: '--jellypilot-color-primary-gradient-end',
    secondary: '--jellypilot-color-secondary',
    secondaryContainer: '--jellypilot-color-secondary-container',
    secondaryGradientEnd: '--jellypilot-color-secondary-gradient-end',
    surface: '--jellypilot-color-surface',
    surfaceContainer: '--jellypilot-color-surface-container',
    surfaceContainerHigh: '--jellypilot-color-surface-container-high',
    surfaceContainerHighest: '--jellypilot-color-surface-container-highest',
    surfaceContainerLow: '--jellypilot-color-surface-container-low',
    surfaceContainerLowest: '--jellypilot-color-surface-container-lowest',
    surfaceTint: '--jellypilot-color-surface-tint',
    surfaceVariant: '--jellypilot-color-surface-variant',
    tertiary: '--jellypilot-color-tertiary',
    tertiaryContainer: '--jellypilot-color-tertiary-container',
    warning: '--jellypilot-color-warning',
    warningContainer: '--jellypilot-color-warning-container',
  },
  font: {
    display: '--jellypilot-font-display',
    mono: '--jellypilot-font-mono',
    sans: '--jellypilot-font-sans',
  },
  space: tokenContract('space', spaceValue),
  fontSize: tokenContract('font-size', fontSizeValue),
  fontWeight: tokenContract('font-weight', fontWeightValue),
  lineHeight: tokenContract('line-height', lineHeightValue),
  borderRadius: tokenContract('radius', borderRadiusValue),
  shadow: tokenContract('shadow', shadowValue),
  zIndex: tokenContract('z-index', zIndexValue),
  duration: tokenContract('duration', durationValue),
  easing: tokenContract('easing', easingValue),
  breakpoint: tokenContract('breakpoint', breakpointValue),
});

createGlobalTheme(':root', vars, {
  color: {
    background: '#05060a',
    brandGlow: '#4f46e5',
    error: '#ff6b7a',
    errorContainer: '#4b1119',
    onBackground: '#f3f6ff',
    onError: '#330006',
    onErrorContainer: '#ffd9de',
    onPrimary: '#ffffff',
    onPrimaryContainer: '#e0e2ff',
    onSecondary: '#0b0a24',
    onSecondaryContainer: '#e0e2ff',
    onSurface: '#f3f6ff',
    onSurfaceVariant: '#aeb8cc',
    onTertiary: '#001f16',
    onTertiaryContainer: '#bfffe8',
    onWarning: '#2a1a00',
    onWarningContainer: '#ffe7a8',
    outline: '#5c6c8c',
    outlineVariant: '#262e42',
    primary: '#4f46e5',
    primaryContainer: '#1b1c3b',
    primaryGradientEnd: '#7a7eff',
    secondary: '#818cf8',
    secondaryContainer: '#1f2152',
    secondaryGradientEnd: '#0b4b60',
    surface: '#0b0d14',
    surfaceContainer: '#111420',
    surfaceContainerHigh: '#161b2a',
    surfaceContainerHighest: '#22293e',
    surfaceContainerLow: '#0a0c12',
    surfaceContainerLowest: '#040508',
    surfaceTint: '#4f46e5',
    surfaceVariant: '#1e2538',
    tertiary: '#4fe3b1',
    tertiaryContainer: '#06382a',
    warning: '#f6c768',
    warningContainer: '#3f2e08',
  },
  font: {
    display: "'Space Grotesk Variable', 'Inter Variable', ui-sans-serif, system-ui, sans-serif",
    mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
    sans: "'Inter Variable', ui-sans-serif, system-ui, sans-serif",
  },
  space: spaceValue,
  fontSize: fontSizeValue,
  fontWeight: fontWeightValue,
  lineHeight: lineHeightValue,
  borderRadius: borderRadiusValue,
  shadow: shadowValue,
  zIndex: zIndexValue,
  duration: durationValue,
  easing: easingValue,
  breakpoint: breakpointValue,
});
