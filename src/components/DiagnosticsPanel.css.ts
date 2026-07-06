import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const root = style({
  display: 'grid',
  gap: vars.space['4'],
});

export const header = sprinkles({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: '3',
  px: '1',
});

export const count = style({
  color: mix(vars.color.onSurfaceVariant, 0.8),
  fontFamily: vars.font.mono,
  fontSize: vars.fontSize['11'],
  fontVariantNumeric: 'tabular-nums',
  fontWeight: vars.fontWeight.semibold,
});

export const checkboxRoot = style([
  sprinkles({
    display: 'inlineFlex',
    alignItems: 'center',
    gap: '2_5',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.95),
    cursor: 'pointer',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
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
    borderRadius: 'lg',
    fontSize: '11',
    lineHeight: 'none',
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

export const indicator = sprinkles({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontWeight: 'black',
});

export const checkboxLabel = style({
  cursor: 'pointer',
  userSelect: 'none',
});

export const log = style([
  sprinkles({
    overflowY: 'auto',
    borderRadius: '2xl',
    p: '3',
    boxShadow: 'inner',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerLowest, 0.6),
    border: `1px solid ${vars.color.outlineVariant}`,
    display: 'grid',
    gap: vars.space['2_5'],
  },
]);

export const compactLog = style({
  maxHeight: '14rem',
});

export const expandedLog = style({
  maxHeight: '24rem',
});

export const empty = style({
  color: mix(vars.color.onSurfaceVariant, 0.6),
  fontFamily: vars.font.mono,
  fontSize: vars.fontSize['12'],
  lineHeight: vars.lineHeight['16'],
  paddingBlock: vars.space['10'],
  textAlign: 'center',
});

export const entry = style([
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    borderRadius: 'xl',
    px: '3_5',
    py: '2',
    color: 'onSurfaceVariant',
    fontSize: '12',
  }),
  {
    background: mix(vars.color.surfaceContainerLowest, 0.7),
    border: `1px solid ${mix(vars.color.outlineVariant, 0.4)}`,
    fontFamily: vars.font.mono,
    lineHeight: vars.lineHeight['20'],
    transitionProperty: 'background-color, border-color',
    selectors: {
      '&:hover': {
        background: mix(vars.color.surfaceContainerLowest, 0.9),
        borderColor: mix(vars.color.outlineVariant, 0.6),
      },
    },
  },
]);

export const entryInner = style({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: `${vars.space['1_5']} ${vars.space['3']}`,
  position: 'relative',
  zIndex: 10,
});

export const time = style({
  color: vars.color.outline,
  fontWeight: vars.fontWeight.semibold,
  userSelect: 'none',
});

export const badge = style({
  border: '1px solid currentColor',
  borderRadius: vars.borderRadius.md,
  fontSize: vars.fontSize['10'],
  fontWeight: vars.fontWeight.bold,
  letterSpacing: '0.05em',
  padding: `${vars.space['0_5']} ${vars.space['2']}`,
  userSelect: 'none',
});

export const badgeTrace = style({
  background: vars.color.surfaceContainerHighest,
  borderColor: mix(vars.color.outlineVariant, 0.4),
  color: vars.color.outline,
});

export const badgeDebug = style({
  background: vars.color.surfaceContainerHighest,
  borderColor: mix(vars.color.outline, 0.3),
  color: vars.color.onSurfaceVariant,
});

export const badgeInfo = style({
  background: mix(vars.color.secondaryContainer, 0.3),
  borderColor: mix(vars.color.secondary, 0.3),
  boxShadow: '0 0 6px rgba(129, 140, 248, 0.1)',
  color: vars.color.secondary,
});

export const badgeWarn = style({
  background: mix(vars.color.warningContainer, 0.3),
  borderColor: mix(vars.color.warning, 0.3),
  boxShadow: '0 0 6px rgba(246, 199, 104, 0.1)',
  color: vars.color.warning,
});

export const badgeError = style({
  background: mix(vars.color.errorContainer, 0.3),
  borderColor: mix(vars.color.error, 0.3),
  boxShadow: '0 0 6px rgba(255, 107, 122, 0.1)',
  color: vars.color.error,
});

export const message = style({
  color: vars.color.onSurfaceVariant,
  fontWeight: vars.fontWeight.medium,
  overflowWrap: 'anywhere',
});

export const actions = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  justifyContent: 'flex-end',
  gap: '3',
  px: '1',
});

export const status = style([
  sprinkles({
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const statusCopied = style({
  color: vars.color.tertiary,
  filter: 'drop-shadow(0 0 6px rgba(79, 227, 177, 0.2))',
});

export const statusFailed = sprinkles({
  color: 'error',
});

export const actionButton = style({
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.xl,
  color: mix(vars.color.onSurfaceVariant, 0.9),
  fontSize: vars.fontSize['11'],
  fontWeight: vars.fontWeight.bold,
  letterSpacing: '0.08em',
  lineHeight: vars.lineHeight['16'],
  textTransform: 'uppercase',
  selectors: {
    '&:hover': {
      background: mix(vars.color.secondary, 0.05),
      borderColor: vars.color.secondary,
    },
  },
});

export const dangerActionButton = style({
  selectors: {
    '&:hover': {
      background: mix(vars.color.error, 0.05),
      borderColor: vars.color.error,
      color: vars.color.error,
    },
  },
});
