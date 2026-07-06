import { style } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';

export const viewport = [
  sprinkles({
    height: 'screenHeight',
    overflow: 'auto',
    width: 'screen',
  }),
  style({
    overscrollBehavior: 'contain',
  }),
].join(' ');

export const content = sprinkles({
  minWidth: 'fit',
});
