import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const root = [
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    borderRadius: '2xl',
    p: '4',
    boxShadow: 'inner',
  }),
  style({
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerHigh, 0.3),
    border: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
  }),
].join(' ');

export const label = [
  sprinkles({
    display: 'block',
    mb: '1',
    color: 'onSurfaceVariant',
    fontSize: '11',
    fontWeight: 'bold',
    lineHeight: '16',
  }),
  style({
    letterSpacing: '0.08em',
    opacity: 0.9,
    textTransform: 'uppercase',
  }),
].join(' ');
