import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';

export const viewport = style([
  sprinkles({
    height: 'screenHeight',
    overflow: 'auto',
    width: 'screen',
  }),
  {
    overscrollBehavior: 'contain',
  },
]);

export const content = sprinkles({
  minWidth: 'fit',
});
