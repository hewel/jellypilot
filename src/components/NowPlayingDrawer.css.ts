import { style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { breakpoints, vars } from '../styles/vars.css';

const mix = (color: string, amount: number) =>
  `color-mix(in srgb, ${color} ${amount}%, transparent)`;

export const trigger = style({
  position: 'relative',
});

export const triggerIcon = style({
  height: vars.space['5'],
  width: vars.space['5'],
});

export const statusDot = recipe({
  base: style({
    borderRadius: vars.borderRadius.full,
    height: vars.space['2'],
    position: 'absolute',
    right: vars.space['1'],
    top: vars.space['1'],
    width: vars.space['2'],
  }),
  variants: {
    status: {
      idle: {
        background: vars.color.outlineVariant,
      },
      offline: {
        background: vars.color.error,
        boxShadow: `0 0 8px ${vars.color.error}`,
      },
      paused: {
        background: vars.color.tertiary,
        boxShadow: `0 0 8px ${vars.color.tertiary}`,
      },
      playing: {
        background: vars.color.tertiary,
        boxShadow: `0 0 8px ${vars.color.tertiary}`,
      },
      unknown: {
        background: vars.color.outlineVariant,
      },
    },
  },
});

export const backdrop = style({
  background: 'rgb(0 0 0 / 0.7)',
  backdropFilter: 'blur(4px)',
  inset: 0,
  position: 'fixed',
  transitionDuration: vars.duration['300'],
  transitionProperty: 'backdrop-filter, background-color, opacity',
  zIndex: vars.zIndex['50'],
  selectors: {
    '&[data-state="closed"]': {
      opacity: 0,
    },
    '&[data-state="open"]': {
      opacity: 1,
    },
  },
});

export const positioner = style({
  display: 'flex',
  inset: 0,
  justifyContent: 'flex-end',
  position: 'fixed',
  zIndex: vars.zIndex['50'],
});

export const content = style({
  backdropFilter: 'blur(24px)',
  background: mix(vars.color.surfaceContainerLow, 60),
  borderLeft: `1px solid ${mix(vars.color.outlineVariant, 30)}`,
  borderTopLeftRadius: '2rem',
  borderBottomLeftRadius: '2rem',
  boxShadow: vars.shadow['2xl'],
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  overflow: 'hidden',
  transitionDuration: vars.duration['200'],
  transitionProperty: 'opacity, transform',
  transitionTimingFunction: vars.easing.standard,
  width: '100%',
  selectors: {
    '&[data-state="closed"]': {
      opacity: 0,
      transform: `translateX(${vars.space['3']})`,
    },
    '&[data-state="open"]': {
      opacity: 1,
      transform: 'translateX(0)',
    },
  },
  '@media': {
    [`screen and (min-width: ${breakpoints.sm})`]: {
      width: '28rem',
    },
  },
});

export const header = style({
  alignItems: 'center',
  borderBottom: `1px solid ${mix(vars.color.outlineVariant, 20)}`,
  display: 'flex',
  justifyContent: 'space-between',
  padding: `${vars.space['4']} ${vars.space['5']}`,
});

export const title = style({
  color: vars.color.onSurface,
  fontSize: vars.fontSize['18'],
  fontWeight: vars.fontWeight.bold,
  lineHeight: vars.lineHeight['24'],
});

export const description = style({
  color: mix(vars.color.onSurfaceVariant, 70),
  fontSize: vars.fontSize['12'],
  lineHeight: vars.lineHeight['16'],
  marginTop: vars.space['0_5'],
});

export const closeIcon = style({
  height: vars.space['5'],
  width: vars.space['5'],
});

export const body = style({
  flex: 1,
  overflowY: 'auto',
  padding: `${vars.space['4']} ${vars.space['5']}`,
});

export const srOnlyClose = style({
  position: 'absolute',
  width: '1px',
  height: '1px',
  overflow: 'hidden',
  clip: 'rect(0 0 0 0)',
});
