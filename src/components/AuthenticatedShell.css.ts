import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

const mix = (color: string, amount: number) =>
  `color-mix(in srgb, ${color} ${amount}%, transparent)`;

export const main = [
  sprinkles({
    color: 'onSurface',
    display: 'flex',
    flexDirection: 'column',
    mx: 'auto',
    width: 'full',
  }),
  style({
    animation: `fadeIn ${vars.duration['300']} ${vars.easing.emphasized} forwards`,
    paddingBottom: '10rem',
  }),
].join(' ');

export const floatingControls = [
  sprinkles({
    alignItems: 'center',
    borderRadius: '3xl',
    boxShadow: '2xl',
    display: 'flex',
    flexDirection: 'column',
    gap: '2',
    p: '1',
    position: 'fixed',
    zIndex: '100',
  }),
  style({
    backdropFilter: 'blur(24px)',
    background: mix(vars.color.surfaceContainerLow, 80),
    border: `1px solid ${mix(vars.color.outlineVariant, 40)}`,
    bottom: vars.space['4'],
    right: vars.space['4'],
  }),
].join(' ');
