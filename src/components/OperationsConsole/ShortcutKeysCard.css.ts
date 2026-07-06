import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const field = style({
  display: 'block',
});

export const input = style({
  color: vars.color.secondary,
  fontFamily: vars.font.mono,
  fontWeight: vars.fontWeight.semibold,
  width: '100%',
});

export const description = style([
  sprinkles({
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);
