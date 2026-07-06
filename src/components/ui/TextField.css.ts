import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const textFieldRoot = style({});

export const textFieldLabel = style([
  sprinkles({
    display: 'block',
    mb: '1',
    ml: '1',
    color: 'onSurfaceVariant',
    fontSize: '12',
    fontWeight: 'bold',
    lineHeight: '16',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    transitionDuration: vars.duration['200'],
    transitionProperty: 'color',
    selectors: {
      [`${textFieldRoot}:focus-within &`]: {
        color: vars.color.primary,
      },
    },
  },
]);

export const textFieldError = style([
  sprinkles({
    mt: '1_5',
    ml: '1',
    color: 'error',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    animation: `fadeIn ${vars.duration['200']} ${vars.easing.emphasized} forwards`,
  },
]);

export const textFieldHint = sprinkles({
  mt: '1_5',
  ml: '1',
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
  opacity: '70',
});

export const fullWidth = sprinkles({ width: 'full' });
