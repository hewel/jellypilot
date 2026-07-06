import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const textFieldRoot = style({});

export const textFieldLabel = [
  sprinkles({
    display: 'block',
    mb: '1',
    ml: '1',
    color: 'onSurfaceVariant',
    fontSize: '12',
    fontWeight: 'bold',
    lineHeight: '16',
  }),
  style({
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    transitionDuration: vars.duration['200'],
    transitionProperty: 'color',
    selectors: {
      [`${textFieldRoot}:focus-within &`]: {
        color: vars.color.primary,
      },
    },
  }),
].join(' ');

export const textFieldError = [
  sprinkles({
    mt: '1_5',
    ml: '1',
    color: 'error',
    fontSize: '12',
    lineHeight: '16',
  }),
  style({
    animation: `fadeIn ${vars.duration['200']} ${vars.easing.emphasized} forwards`,
  }),
].join(' ');

export const textFieldHint = sprinkles({
  mt: '1_5',
  ml: '1',
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
  opacity: '70',
});

export const fullWidth = sprinkles({ width: 'full' });
