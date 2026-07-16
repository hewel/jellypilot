import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const card = style([
  sprinkles({
    position: 'relative',
    mx: 'auto',
    overflow: 'hidden',
    boxShadow: '2xl',
  }),
]);

export const accent = style({
  background: `linear-gradient(90deg, transparent, ${mix(vars.color.primary, 0.55)}, transparent)`,
  height: '2px',
  left: 0,
  position: 'absolute',
  top: 0,
  width: '100%',
});

export const stack7 = style({
  display: 'grid',
  gap: vars.space['7'],
});

export const sectionTitle = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    gap: '2_5',
    color: 'onSurface',
    fontSize: '24',
    lineHeight: '32',
    fontWeight: 'bold',
  }),
  {
    fontFamily: vars.font.display,
  },
]);

export const titleBar = style({
  background: vars.color.primary,
  borderRadius: vars.borderRadius.md,
  height: vars.space['5'],
  width: vars.space['1_5'],
});

export const description = sprinkles({
  mt: '1_5',
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
});

export const serverGrid = style({
  display: 'grid',
  gap: vars.space['3'],
  gridTemplateColumns: '1fr',
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'auto minmax(0, 1fr)',
    },
  },
});

export const segmented = style([
  sprinkles({
    display: 'grid',
    borderRadius: '2xl',
    p: '1',
  }),
  {
    background: mix(vars.color.surfaceContainerHigh, 0.4),
    border: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const segmented2 = style({
  gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
});

export const segmented1 = style({
  gridTemplateColumns: '1fr',
});

export const segment = style([
  sprinkles({
    borderRadius: 'xl',
    px: '4',
    py: '3',
    fontSize: '14',
    lineHeight: '20',
    fontWeight: 'semibold',
  }),
  {
    background: 'transparent',
    border: 0,
    cursor: 'pointer',
    letterSpacing: 0,
    textTransform: 'uppercase',
    transitionDuration: vars.duration['300'],
    transitionProperty: 'background-color, color, box-shadow, transform',
    selectors: {
      '&:hover': {
        background: mix(vars.color.surfaceContainerHighest, 0.4),
        color: vars.color.onSurface,
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
      '&:disabled': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const segmentIdle = style({
  color: vars.color.onSurfaceVariant,
});

export const segmentSelected = style({
  background: `linear-gradient(90deg, ${vars.color.primary}, ${vars.color.primaryGradientEnd})`,
  boxShadow: `0 4px 8px ${mix(vars.color.primary, 0.25)}`,
  color: vars.color.onPrimary,
  fontWeight: vars.fontWeight.bold,
});

export const srOnly = style({
  border: 0,
  clip: 'rect(0 0 0 0)',
  height: '1px',
  margin: '-1px',
  overflow: 'hidden',
  padding: 0,
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: '1px',
});

export const preview = style([
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    borderRadius: '2xl',
    p: '4',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerLowest, 0.4),
    border: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const previewStripe = style({
  background: vars.color.secondary,
  bottom: 0,
  left: 0,
  position: 'absolute',
  top: 0,
  width: '3px',
});

export const overline = style([
  sprinkles({
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.9),
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const previewValue = style([
  sprinkles({
    mt: '1',
    color: 'onSurfaceVariant',
    fontSize: '14',
    lineHeight: '20',
  }),
  {
    fontFamily: vars.font.mono,
    overflowWrap: 'anywhere',
  },
]);

export const previewReady = style({
  color: vars.color.secondary,
  filter: 'drop-shadow(0 0 8px rgba(129, 140, 248, 0.15))',
  fontWeight: vars.fontWeight.semibold,
});

export const previewEmpty = style({
  color: vars.color.warning,
});

export const fieldBlock = style({
  display: 'block',
});

export const label = style([
  sprinkles({
    display: 'block',
    mb: '1_5',
    color: 'onSurfaceVariant',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const tabsList = style({
  marginBottom: vars.space['6'],
});

export const quickPanel = style([
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    borderRadius: '3xl',
    p: '6',
    textAlign: 'center',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.secondaryContainer, 0.2),
    border: `1px solid ${mix(vars.color.secondary, 0.25)}`,
    transitionProperty: 'color',
  },
]);

export const quickPanelGlow = style({
  background: `linear-gradient(to bottom, ${mix(vars.color.secondary, 0.05)}, transparent)`,
  inset: 0,
  pointerEvents: 'none',
  position: 'absolute',
});

export const radar = style([
  sprinkles({
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    mx: 'auto',
    mb: '4',
    width: '20',
    height: '20',
    borderRadius: 'full',
  }),
  {
    background: mix(vars.color.secondary, 0.05),
    border: `1px solid ${mix(vars.color.secondary, 0.2)}`,
  },
]);

export const radarRing = style({
  animation: 'radar-pulse 2.2s cubic-bezier(0.2, 0.8, 0.2, 1) infinite',
  border: `1px solid ${mix(vars.color.secondary, 0.4)}`,
  borderRadius: vars.borderRadius.full,
  inset: 0,
  position: 'absolute',
});

export const radarRing2 = style({
  animationDelay: '0.7s',
  borderColor: mix(vars.color.secondary, 0.3),
});

export const radarRing3 = style({
  animationDelay: '1.4s',
  borderColor: mix(vars.color.secondary, 0.2),
});

export const towerIcon = style({
  color: vars.color.secondary,
  filter: 'drop-shadow(0 0 8px rgba(129, 140, 248, 0.4))',
  height: vars.space['9'],
  width: vars.space['9'],
});

export const pulse = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
});

export const quickText = sprinkles({
  color: 'onSecondaryContainer',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'medium',
});

export const quickHint = style([
  sprinkles({
    mt: '2',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);

export const codeBox = style([
  sprinkles({
    display: 'inlineFlex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    mt: '6',
    borderRadius: '2xl',
    px: '6',
    py: '3_5',
    boxShadow: 'inner',
  }),
  {
    background: mix(vars.color.surfaceContainerLowest, 0.8),
    border: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const codeLabel = style([
  sprinkles({
    mb: '1',
    fontSize: '10',
    fontWeight: 'bold',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
    letterSpacing: '0.2em',
    textTransform: 'uppercase',
  },
]);

export const code = style({
  color: vars.color.secondary,
  filter: 'drop-shadow(0 0 10px rgba(129, 140, 248, 0.55))',
  fontFamily: vars.font.mono,
  fontSize: vars.fontSize['36'],
  fontVariantNumeric: 'tabular-nums',
  fontWeight: vars.fontWeight.bold,
  letterSpacing: '0.25em',
  lineHeight: vars.lineHeight['44'],
  paddingLeft: '0.25em',
});

export const waiting = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '2',
    mt: '5',
    color: 'secondary',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const waitingDot = style({
  background: vars.color.secondary,
  borderRadius: vars.borderRadius.full,
  boxShadow: '0 0 8px #818cf8',
  height: vars.space['2'],
  width: vars.space['2'],
});

export const stack4 = style({
  display: 'grid',
  gap: vars.space['4'],
});

export const remember = style([
  sprinkles({
    display: 'inlineFlex',
    alignItems: 'center',
    gap: '2_5',
    mt: '2_5',
    color: 'onSurface',
    fontSize: '14',
    lineHeight: '20',
  }),
  {
    cursor: 'pointer',
    transitionProperty: 'opacity',
    userSelect: 'none',
    verticalAlign: 'top',
    selectors: {
      '&:disabled': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const checkbox = style([
  sprinkles({
    display: 'inlineFlex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: '0',
    color: 'onPrimary',
    fontSize: '11',
    lineHeight: 'none',
    borderRadius: 'lg',
  }),
  {
    background: vars.color.surfaceContainerHigh,
    border: `1px solid ${vars.color.outline}`,
    height: '1.375rem',
    transitionDuration: vars.duration['200'],
    transitionProperty: 'background-color, border-color, box-shadow',
    width: '1.375rem',
    selectors: {
      '&:hover': {
        borderColor: mix(vars.color.primary, 0.6),
      },
      '&[data-state="checked"], &[data-state="indeterminate"]': {
        background: `linear-gradient(135deg, ${vars.color.primary}, ${vars.color.primaryGradientEnd})`,
        borderColor: vars.color.primary,
      },
      '&[data-focus-visible]': {
        boxShadow: `0 0 0 2px ${mix(vars.color.primary, 0.5)}`,
        outline: 'none',
      },
    },
  },
]);

export const checkboxIndicator = sprinkles({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontWeight: 'black',
});

export const checkboxLabel = style({
  cursor: 'pointer',
  fontWeight: vars.fontWeight.medium,
  transitionProperty: 'color',
  userSelect: 'none',
  selectors: {
    '&:hover': {
      color: vars.color.onSurfaceVariant,
    },
  },
});

export const alert = style([
  sprinkles({
    display: 'flex',
    alignItems: 'flex-start',
    gap: '3',
    borderRadius: '2xl',
    p: '4',
    color: 'onErrorContainer',
  }),
  {
    background: mix(vars.color.errorContainer, 0.2),
    border: `1px solid ${mix(vars.color.error, 0.3)}`,
  },
]);

export const alertIcon = sprinkles({
  mt: '0_5',
  width: '5',
  height: '5',
  flexShrink: '0',
  color: 'error',
});

export const alertTitle = sprinkles({
  color: 'error',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'bold',
});

export const alertMessage = sprinkles({
  mt: '0_5',
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
});

export const main = style({
  maxWidth: '48rem',
  position: 'relative',
  width: '100%',
  zIndex: 10,
});

export const hero = style([
  sprinkles({
    position: 'relative',
    mb: '8',
    textAlign: 'center',
  }),
]);

export const heroIconWrap = style([
  sprinkles({
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    mx: 'auto',
    mb: '6',
    width: '24',
    height: '24',
    borderRadius: 'full',
  }),
  {
    background: mix(vars.color.primary, 0.05),
    border: `1px solid ${mix(vars.color.primary, 0.2)}`,
    boxShadow: '0 0 30px rgba(79, 70, 229, 0.2)',
  },
]);

export const heroRing = style({
  animation: 'ping 1s cubic-bezier(0, 0, 0.2, 1) infinite',
  border: `1px solid ${mix(vars.color.primary, 0.3)}`,
  borderRadius: vars.borderRadius.full,
  inset: 0,
  opacity: 0.25,
  position: 'absolute',
});

export const heroPulseRing = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  border: `1px solid ${mix(vars.color.secondary, 0.25)}`,
  borderRadius: vars.borderRadius.full,
  inset: vars.space['2'],
  position: 'absolute',
});

export const heroOrbit = style({
  animation: 'spin 60s linear infinite',
  border: `1px dashed ${mix(vars.color.primary, 0.1)}`,
  borderRadius: vars.borderRadius.full,
  inset: vars.space['4'],
  position: 'absolute',
});

export const heroIcon = style({
  color: vars.color.primary,
  filter: 'drop-shadow(0 0 12px rgba(79, 70, 229, 0.55))',
  height: vars.space['10'],
  width: vars.space['10'],
});

export const badge = style([
  sprinkles({
    display: 'inlineFlex',
    alignItems: 'center',
    gap: '2_5',
    mb: '3_5',
    borderRadius: 'full',
    px: '3_5',
    py: '1',
  }),
  {
    background: mix(vars.color.secondary, 0.05),
    border: `1px solid ${mix(vars.color.secondary, 0.2)}`,
  },
]);

export const badgeDot = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: vars.color.secondary,
  borderRadius: vars.borderRadius.full,
  boxShadow: '0 0 8px #818cf8',
  height: vars.space['1_5'],
  width: vars.space['1_5'],
});

export const badgeText = style([
  sprinkles({
    color: 'secondary',
    fontSize: '10',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.18em',
    textTransform: 'uppercase',
  },
]);

export const appTitle = style({
  color: vars.color.onSurface,
  fontFamily: vars.font.display,
  fontSize: vars.fontSize['45'],
  fontWeight: vars.fontWeight.bold,
  letterSpacing: 0,
  lineHeight: vars.lineHeight['52'],
});

export const appDescription = style([
  sprinkles({
    mx: 'auto',
    mt: '2',
    color: 'onSurfaceVariant',
    fontSize: '16',
    lineHeight: '24',
  }),
  {
    maxWidth: '28rem',
  },
]);
