import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

export const version = style([
  sprinkles({
    color: 'onSurfaceVariant',
    fontSize: '11',
    fontWeight: 'bold',
    lineHeight: '16',
    mt: '1',
  }),
  {
    fontFamily: vars.font.mono,
    letterSpacing: '0.08em',
    opacity: 0.5,
    textTransform: 'uppercase',
  },
]);
