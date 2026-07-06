import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const root = sprinkles({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  pb: '4',
});

export const title = [
  sprinkles({
    color: 'onSurface',
    fontSize: '32',
    fontWeight: 'bold',
    lineHeight: '40',
  }),
  style({
    fontFamily: vars.font.display,
  }),
].join(' ');

export const description = sprinkles({
  mt: '1',
  color: 'onSurfaceVariant',
  fontSize: '16',
  lineHeight: '24',
});
