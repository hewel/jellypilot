import { style, styleVariants } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const sectionIcon = styleVariants({
  primary: {
    color: vars.color.primary,
    filter: 'drop-shadow(0 0 8px rgba(79, 70, 229, 0.4))',
    height: vars.space['5'],
    width: vars.space['5'],
  },
  secondary: {
    color: vars.color.secondary,
    filter: 'drop-shadow(0 0 8px rgba(129, 140, 248, 0.4))',
    height: vars.space['5'],
    width: vars.space['5'],
  },
  plain: {
    height: vars.space['6'],
    width: vars.space['6'],
  },
});

export const grid3 = style({
  display: 'grid',
  gap: vars.space['4'],
  gridTemplateColumns: '1fr',
  '@media': {
    'screen and (min-width: 768px)': {
      gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
    },
  },
});

export const tile = style([
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    borderRadius: '2xl',
    p: '4',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerHigh, 0.3),
    border: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
  },
]);

export const span2 = style({
  '@media': {
    'screen and (min-width: 768px)': {
      gridColumn: 'span 2 / span 2',
    },
  },
});

export const tileWatermark = style({
  opacity: 0.05,
  padding: vars.space['3'],
  position: 'absolute',
  right: 0,
  top: 0,
});

export const watermarkIcon = sprinkles({
  width: '12',
  height: '12',
});

export const overline = style([
  sprinkles({
    color: 'onSurfaceVariant',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const value = style([
  sprinkles({
    mt: '1_5',
    color: 'onSurface',
    fontSize: '16',
    lineHeight: '24',
    fontWeight: 'bold',
  }),
  {
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const monoValue = style([
  sprinkles({
    mt: '1_5',
    color: 'secondary',
    fontSize: '14',
    lineHeight: '20',
  }),
  {
    fontFamily: vars.font.mono,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const bodyText = style([
  sprinkles({
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);

export const warning = sprinkles({
  mt: '2',
  display: 'flex',
  alignItems: 'flex-start',
  gap: '2',
  color: 'warning',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
});

export const warningIcon = style({
  flexShrink: 0,
  height: vars.space['3_5'],
  marginTop: vars.space['0_5'],
  width: vars.space['3_5'],
});

export const actionRow = sprinkles({
  mt: '6',
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '3',
});

export const refreshButton = style({
  background: mix(vars.color.surfaceContainerHigh, 0.2),
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.xl,
  marginLeft: 'auto',
  selectors: {
    '&:hover': {
      borderColor: vars.color.secondary,
      color: vars.color.secondary,
    },
  },
});

export const mutedOutlinedButton = style({
  color: vars.color.onSurfaceVariant,
  selectors: {
    '&:hover': {
      borderColor: mix(vars.color.primary, 0.5),
      color: vars.color.onSurface,
    },
  },
});

export const stack4 = style({
  display: 'grid',
  gap: vars.space['4'],
});

export const fieldset = style({
  border: 0,
  display: 'grid',
  gap: vars.space['3'],
  gridTemplateColumns: '1fr',
  margin: 0,
  padding: 0,
});

export const choice = style([
  sprinkles({
    borderRadius: '2xl',
    px: '4',
    py: '3',
    textAlign: 'left',
  }),
  {
    backdropFilter: 'blur(4px)',
    border: `1px solid ${vars.color.outlineVariant}`,
    cursor: 'pointer',
    transitionDuration: vars.duration['300'],
    transitionProperty: 'background-color, border-color, box-shadow, transform',
    selectors: {
      '&:active': {
        transform: 'scale(0.96)',
      },
      '&:hover': {
        background: mix(vars.color.surfaceContainerHigh, 0.6),
        borderColor: mix(vars.color.primary, 0.5),
      },
    },
  },
]);

export const choiceIdle = style({
  background: mix(vars.color.surfaceContainerHigh, 0.4),
  color: vars.color.onSurface,
});

export const choiceSelected = style({
  background: mix(vars.color.primaryContainer, 0.35),
  borderColor: vars.color.primary,
  boxShadow: '0 0 15px rgba(79, 70, 229, 0.25)',
  color: vars.color.onPrimaryContainer,
  fontWeight: vars.fontWeight.semibold,
});

export const choiceTitle = sprinkles({
  display: 'block',
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const choiceDescription = style([
  sprinkles({
    display: 'block',
    mt: '1',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
    opacity: 0.8,
  },
]);

export const saving = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    gap: '1_5',
    color: 'secondary',
    fontSize: '14',
    lineHeight: '20',
    fontWeight: 'semibold',
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  },
]);

export const pingDot = style({
  animation: 'ping 1s cubic-bezier(0, 0, 0.2, 1) infinite',
  background: vars.color.secondary,
  borderRadius: vars.borderRadius.full,
  boxShadow: '0 0 8px #818cf8',
  height: vars.space['1_5'],
  width: vars.space['1_5'],
});

export const errorPanel = style([
  sprinkles({
    borderRadius: '2xl',
    px: '4',
    py: '3',
    color: 'onErrorContainer',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'semibold',
  }),
  {
    background: mix(vars.color.errorContainer, 0.2),
    border: `1px solid ${mix(vars.color.error, 0.3)}`,
  },
]);
