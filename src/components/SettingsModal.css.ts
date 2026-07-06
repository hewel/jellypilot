import { style } from '@vanilla-extract/css';

import { vars } from '../styles/vars.css';

const mix = (color: string, amount: number) =>
  `color-mix(in srgb, ${color} ${amount}%, transparent)`;

export const trigger = style({
  boxShadow: `${vars.shadow['2xl']}, 0 25px 50px -12px ${mix(vars.color.secondary, 45)}`,
  height: '3.25rem',
  padding: '0 !important',
  width: '3.25rem',
});

export const triggerIcon = style({
  height: vars.space['5'],
  width: vars.space['5'],
});

export const backdrop = style({
  background: 'rgb(0 0 0 / 0.7)',
  backdropFilter: 'blur(4px)',
  inset: 0,
  position: 'fixed',
  transitionDuration: vars.duration['300'],
  transitionProperty: 'backdrop-filter, background-color, opacity',
  zIndex: vars.zIndex['40'],
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
  flexDirection: 'column',
  height: '100%',
  inset: 0,
  overflow: 'hidden',
  position: 'fixed',
  width: '100%',
  zIndex: vars.zIndex['40'],
});

export const content = style({
  backdropFilter: 'blur(24px)',
  background: mix(vars.color.surfaceContainerLow, 60),
  border: `1px solid ${mix(vars.color.outlineVariant, 30)}`,
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  outline: 'none',
  overflow: 'hidden',
  transitionDuration: vars.duration['200'],
  transitionProperty: 'opacity, transform',
  transitionTimingFunction: vars.easing.standard,
  width: '100%',
  selectors: {
    '&[data-state="closed"]': {
      opacity: 0,
      transform: `translateY(${vars.space['1']})`,
    },
    '&[data-state="open"]': {
      opacity: 1,
      transform: 'translateY(0)',
    },
  },
});

export const header = style({
  alignItems: 'center',
  backdropFilter: 'blur(24px)',
  background: mix(vars.color.surfaceContainerLow, 70),
  borderBottom: `1px solid ${mix(vars.color.outlineVariant, 40)}`,
  display: 'flex',
  gap: vars.space['3'],
  justifyContent: 'space-between',
  padding: `${vars.space['4']} ${vars.space['5']}`,
});

export const title = style({
  color: vars.color.onSurface,
  fontSize: vars.fontSize['22'],
  fontWeight: vars.fontWeight.bold,
  lineHeight: vars.lineHeight['28'],
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
  minHeight: 0,
  overflowY: 'auto',
  padding: `${vars.space['4']} ${vars.space['5']}`,
});
