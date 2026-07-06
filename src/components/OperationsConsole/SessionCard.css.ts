import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const card = style({
  background: mix(vars.color.errorContainer, 0.05),
  borderColor: mix(vars.color.error, 0.2),
  selectors: {
    '&:hover': {
      borderColor: mix(vars.color.error, 0.45),
    },
  },
});

export const header = sprinkles({
  display: 'flex',
  alignItems: 'flex-start',
  gap: '3',
});

export const cardIcon = style({
  color: vars.color.error,
  filter: 'drop-shadow(0 0 8px rgba(255, 107, 122, 0.4))',
  height: vars.space['5'],
  marginTop: vars.space['1'],
  width: vars.space['5'],
});

export const title = sprinkles({
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const description = style([
  sprinkles({
    mt: '1',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);

export const signOutButton = style({
  borderColor: mix(vars.color.error, 0.55),
  color: vars.color.error,
  marginTop: vars.space['5'],
  width: '100%',
  selectors: {
    '&:hover': {
      background: mix(vars.color.error, 0.1),
      borderColor: vars.color.error,
    },
  },
});

export const backdrop = style({
  backdropFilter: 'blur(4px)',
  background: 'rgb(0 0 0 / 0.7)',
  inset: 0,
  position: 'fixed',
  transitionDuration: vars.duration['300'],
  transitionProperty: 'backdrop-filter, background-color, opacity',
  zIndex: 50,
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
  p: '4',
  zIndex: '50',
});

export const positionerFill = style({
  inset: 0,
});

export const content = style([
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    p: '6',
    boxShadow: '2xl',
  }),
  {
    backdropFilter: 'blur(24px)',
    background: mix(vars.color.surfaceContainerLow, 0.45),
    border: `1px solid ${mix(vars.color.error, 0.3)}`,
    borderRadius: '2rem',
    maxWidth: '28rem',
    transitionDuration: vars.duration['300'],
    transitionProperty: 'background-color, border-color, box-shadow, opacity, transform',
    selectors: {
      '&:hover': {
        background: mix(vars.color.surfaceContainerLow, 0.6),
        borderColor: mix(vars.color.primary, 0.35),
      },
      '&[data-state="closed"]': {
        opacity: 0,
        transform: 'translateY(0.25rem)',
      },
      '&[data-state="open"]': {
        opacity: 1,
        transform: 'translateY(0)',
      },
    },
  },
]);

export const glow = style({
  background: `linear-gradient(90deg, transparent, ${mix(vars.color.error, 0.6)}, transparent)`,
  height: '3px',
  left: 0,
  position: 'absolute',
  top: 0,
  width: '100%',
});

export const dialogTitle = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  color: 'onSurface',
  fontSize: '22',
  lineHeight: '28',
  fontWeight: 'bold',
});

export const dialogIcon = sprinkles({
  width: '6',
  height: '6',
  color: 'error',
});

export const dialogDescription = style([
  sprinkles({
    mt: '3',
    fontSize: '14',
    lineHeight: '20',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.9),
  },
]);

export const actions = style({
  display: 'flex',
  flexDirection: 'column-reverse',
  gap: vars.space['3'],
  marginTop: vars.space['6'],
  '@media': {
    'screen and (min-width: 640px)': {
      flexDirection: 'row',
      justifyContent: 'flex-end',
    },
  },
});

export const dangerButton = style({
  borderColor: mix(vars.color.error, 0.6),
  color: vars.color.error,
  selectors: {
    '&:hover': {
      background: mix(vars.color.error, 0.1),
    },
  },
});
