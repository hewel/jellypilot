/**
 * Canonical design-token literals for JellyPilot.
 * Consumed by panda.config.ts (value owner) and by tests.
 * Do not import this from application components.
 */

export const rawColors = {
  neutral: {
    '0': '#ffffff',
    '50': '#f3f6ff',
    '300': '#aeb8cc',
    '500': '#5c6c8c',
    '700': '#262e42',
    '750': '#22293e',
    '800': '#1e2538',
    '850': '#161b2a',
    '900': '#111420',
    '925': '#0b0d14',
    '950': '#0a0c12',
    '975': '#05060a',
    '1000': '#040508',
  },
  indigo: {
    '50': '#e0e2ff',
    '300': '#818cf8',
    '600': '#4f46e5',
    '900': '#1f2152',
    '950': '#1b1c3b',
    '1000': '#0b0a24',
  },
  teal: {
    '50': '#bfffe8',
    '400': '#4fe3b1',
    '900': '#06382a',
    '1000': '#001f16',
  },
  amber: {
    '50': '#ffe7a8',
    '400': '#f6c768',
    '900': '#3f2e08',
    '1000': '#2a1a00',
  },
  red: {
    '50': '#ffd9de',
    '400': '#ff6b7a',
    '900': '#4b1119',
    '1000': '#330006',
  },
} as const;

/** Semantic roles → raw palette path (Panda token reference). */
export const semanticColorRefs = {
  background: 'colors.neutral.975',
  error: 'colors.red.400',
  errorContainer: 'colors.red.900',
  onBackground: 'colors.neutral.50',
  onError: 'colors.red.1000',
  onErrorContainer: 'colors.red.50',
  onPrimary: 'colors.neutral.0',
  onPrimaryContainer: 'colors.indigo.50',
  onSecondary: 'colors.indigo.1000',
  onSecondaryContainer: 'colors.indigo.50',
  onSurface: 'colors.neutral.50',
  onSurfaceVariant: 'colors.neutral.300',
  onTertiary: 'colors.teal.1000',
  onTertiaryContainer: 'colors.teal.50',
  onWarning: 'colors.amber.1000',
  onWarningContainer: 'colors.amber.50',
  outline: 'colors.neutral.500',
  outlineVariant: 'colors.neutral.700',
  primary: 'colors.indigo.600',
  primaryContainer: 'colors.indigo.950',
  secondary: 'colors.indigo.300',
  secondaryContainer: 'colors.indigo.900',
  surface: 'colors.neutral.925',
  surfaceContainer: 'colors.neutral.900',
  surfaceContainerHigh: 'colors.neutral.850',
  surfaceContainerHighest: 'colors.neutral.750',
  surfaceContainerLow: 'colors.neutral.950',
  surfaceContainerLowest: 'colors.neutral.1000',
  surfaceTint: 'colors.indigo.600',
  surfaceVariant: 'colors.neutral.800',
  tertiary: 'colors.teal.400',
  tertiaryContainer: 'colors.teal.900',
  warning: 'colors.amber.400',
  warningContainer: 'colors.amber.900',
} as const;

export const semanticColorHex = {
  background: rawColors.neutral['975'],
  error: rawColors.red['400'],
  errorContainer: rawColors.red['900'],
  onBackground: rawColors.neutral['50'],
  onError: rawColors.red['1000'],
  onErrorContainer: rawColors.red['50'],
  onPrimary: rawColors.neutral['0'],
  onPrimaryContainer: rawColors.indigo['50'],
  onSecondary: rawColors.indigo['1000'],
  onSecondaryContainer: rawColors.indigo['50'],
  onSurface: rawColors.neutral['50'],
  onSurfaceVariant: rawColors.neutral['300'],
  onTertiary: rawColors.teal['1000'],
  onTertiaryContainer: rawColors.teal['50'],
  onWarning: rawColors.amber['1000'],
  onWarningContainer: rawColors.amber['50'],
  outline: rawColors.neutral['500'],
  outlineVariant: rawColors.neutral['700'],
  primary: rawColors.indigo['600'],
  primaryContainer: rawColors.indigo['950'],
  secondary: rawColors.indigo['300'],
  secondaryContainer: rawColors.indigo['900'],
  surface: rawColors.neutral['925'],
  surfaceContainer: rawColors.neutral['900'],
  surfaceContainerHigh: rawColors.neutral['850'],
  surfaceContainerHighest: rawColors.neutral['750'],
  surfaceContainerLow: rawColors.neutral['950'],
  surfaceContainerLowest: rawColors.neutral['1000'],
  surfaceTint: rawColors.indigo['600'],
  surfaceVariant: rawColors.neutral['800'],
  tertiary: rawColors.teal['400'],
  tertiaryContainer: rawColors.teal['900'],
  warning: rawColors.amber['400'],
  warningContainer: rawColors.amber['900'],
} as const;

export const fonts = {
  display: "'Space Grotesk Variable', 'Inter Variable', ui-sans-serif, system-ui, sans-serif",
  mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
  sans: "'Inter Variable', ui-sans-serif, system-ui, sans-serif",
} as const;

export const spacing = {
  '0': '0',
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
  xs: '0.75rem',
  sm: '0.875rem',
  md: '1rem',
  lg: '1.125rem',
  xl: '1.25rem',
  '2xl': '1.5rem',
  '3xl': '1.875rem',
  '4xl': '2.25rem',
  '5xl': '3rem',
  '6xl': '3.75rem',
  '7xl': '4.5rem',
  '8xl': '6rem',
  '9xl': '8rem',
} as const;

export const fontSizes = {
  '10': '0.625rem',
  '11': '0.6875rem',
  '12': '0.75rem',
  '13': '0.8125rem',
  '14': '0.875rem',
  '15': '0.9375rem',
  '16': '1rem',
  '18': '1.125rem',
  '20': '1.25rem',
  '22': '1.375rem',
  '24': '1.5rem',
  '28': '1.75rem',
  '32': '2rem',
  '36': '2.25rem',
  '45': '2.8125rem',
} as const;

export const lineHeights = {
  none: '1',
  tight: '1.25',
  snug: '1.375',
  normal: '1.5',
  relaxed: '1.625',
  loose: '2',
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
} as const;

export const fontWeights = {
  normal: '400',
  medium: '500',
  semibold: '600',
  bold: '700',
  extrabold: '800',
  black: '900',
} as const;

export const radii = {
  none: '0',
  sm: '0.125rem',
  md: '0.375rem',
  lg: '0.5rem',
  xl: '0.75rem',
  '2xl': '1rem',
  '3xl': '1.5rem',
  '4xl': '2rem',
  full: '9999px',
} as const;

export const shadows = {
  none: '0 0 rgb(0 0 0 / 0)',
  sm: '0 1px 2px 0 rgb(0 0 0 / 0.2)',
  md: '0 4px 8px -2px rgb(0 0 0 / 0.35)',
  lg: '0 10px 18px -6px rgb(0 0 0 / 0.45)',
  xl: '0 18px 30px -10px rgb(0 0 0 / 0.55)',
  '2xl': '0 25px 50px -12px rgb(0 0 0 / 0.65)',
  inner: 'inset 0 2px 4px 0 rgb(0 0 0 / 0.28)',
} as const;

export const zIndex = {
  auto: 'auto',
  '0': '0',
  '10': '10',
  '20': '20',
  '40': '40',
  '50': '50',
  '60': '60',
  '100': '100',
  behind: '-1',
} as const;

export const letterSpacings = {
  '0': '0',
  '5': '0.05em',
  '8': '0.08em',
  '18': '0.18em',
  '20': '0.2em',
  '25': '0.25em',
} as const;

export const durations = {
  none: '0s',
  '75': '75ms',
  '100': '100ms',
  '150': '150ms',
  '200': '200ms',
  '300': '300ms',
  '500': '500ms',
  '700': '700ms',
  '1000': '1000ms',
} as const;

export const easings = {
  standard: 'cubic-bezier(0.2, 0, 0, 1)',
  emphasized: 'cubic-bezier(0.16, 1, 0.3, 1)',
  inOut: 'cubic-bezier(0.4, 0, 0.6, 1)',
  linear: 'linear',
} as const;

/** Compile-time breakpoints for Panda responsive conditions. */
export const breakpoints = {
  sm: '640px',
  md: '768px',
  lg: '1024px',
  xl: '1280px',
  '2xl': '1536px',
} as const;
