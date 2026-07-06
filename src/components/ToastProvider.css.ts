import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

export const viewport = [
  sprinkles({
    display: 'flex',
    flexDirection: 'column',
    gap: '2',
    position: 'fixed',
    zIndex: '50',
  }),
  style({
    bottom: vars.space['4'],
    pointerEvents: 'none',
    right: vars.space['4'],
  }),
].join(' ');
