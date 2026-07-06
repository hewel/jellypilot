import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const backdrop = style({
  backdropFilter: 'blur(4px)',
  background: 'rgb(0 0 0 / 0.7)',
  inset: 0,
  position: 'fixed',
  transitionDuration: vars.duration['300'],
  transitionProperty: 'backdrop-filter, background-color, opacity',
  zIndex: 60,
  selectors: {
    '&[data-state="closed"]': {
      opacity: 0,
    },
    '&[data-state="open"]': {
      opacity: 1,
    },
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
  maxWidth: '42rem',
  outline: 'none',
  position: 'relative',
  width: '100%',
});

export const card = style({
  background: `color-mix(in srgb, ${vars.color.secondaryContainer} 10%, transparent)`,
  borderColor: `color-mix(in srgb, ${vars.color.secondary} 40%, transparent)`,
  display: 'grid',
  gap: vars.space['4'],
});

export const eyebrow = style([
  sprinkles({
    color: 'secondary',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const title = sprinkles({
  color: 'onSurface',
  fontSize: '22',
  lineHeight: '28',
  fontWeight: 'bold',
});

export const fields = style({
  display: 'grid',
  gap: vars.space['4'],
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
    },
  },
});

export const actions = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  justifyContent: 'flex-end',
  gap: '3',
});

export const closeButton = style([
  sprinkles({
    display: 'inlineFlex',
    minHeight: '11',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '2',
    borderRadius: 'full',
    px: '5',
    py: '3',
    color: 'onSurface',
    fontSize: '14',
    lineHeight: '20',
    fontWeight: 'bold',
  }),
  {
    background: 'transparent',
    border: `1px solid ${vars.color.outline}`,
    cursor: 'pointer',
    transitionDuration: vars.duration['200'],
    transitionProperty: 'background-color, border-color, color, transform',
    userSelect: 'none',
    selectors: {
      '&:hover': {
        background: `color-mix(in srgb, ${vars.color.primary} 5%, transparent)`,
        borderColor: vars.color.primary,
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
      '&:disabled': {
        opacity: 0.5,
        pointerEvents: 'none',
      },
    },
  },
]);

export const icon = sprinkles({
  width: '4',
  height: '4',
});

export const pillButton = style({
  borderRadius: vars.borderRadius.full,
});

export const playIcon = style([
  sprinkles({
    width: '4',
    height: '4',
  }),
  {
    fill: 'currentColor',
  },
]);
