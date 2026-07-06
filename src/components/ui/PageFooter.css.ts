import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';

export const root = sprinkles({
  py: '8',
  textAlign: 'center',
});

export const text = [
  sprinkles({
    color: 'onSurfaceVariant',
    fontSize: '12',
    lineHeight: '16',
  }),
  style({ opacity: 0.7 }),
].join(' ');
