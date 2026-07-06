import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';

export const root = sprinkles({
  py: '8',
  textAlign: 'center',
});

export const text = style([
  sprinkles({
    color: 'onSurfaceVariant',
    fontSize: '12',
    lineHeight: '16',
  }),
  { opacity: 0.7 },
]);
