import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const stack = style({
  display: 'grid',
  gap: vars.space['6'],
});

export const backdrop = style({
  backdropFilter: 'blur(4px)',
  background: 'rgb(0 0 0 / 0.7)',
  inset: 0,
  position: 'fixed',
  transitionDuration: vars.duration['300'],
  transitionProperty: 'backdrop-filter, background-color, opacity',
  zIndex: 60,
  selectors: {
    '&[data-state="closed"]': { opacity: 0 },
    '&[data-state="open"]': { opacity: 1 },
  },
});

export const positioner = sprinkles({
  position: 'fixed',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  overflowY: 'auto',
  p: '4',
  zIndex: '60',
});

export const positionerFill = style({
  inset: 0,
});

export const content = style({
  maxWidth: '48rem',
  outline: 'none',
  position: 'relative',
  width: '100%',
});

export const closeButton = style({
  backdropFilter: 'blur(8px)',
  background: mix(vars.color.surfaceContainerHigh, 0.8),
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.xl,
  boxShadow: vars.shadow.lg,
  color: vars.color.onSurfaceVariant,
  position: 'absolute',
  right: vars.space['4'],
  top: vars.space['4'],
  zIndex: 10,
  selectors: {
    '&:hover': {
      borderColor: vars.color.secondary,
      color: vars.color.secondary,
    },
  },
});
