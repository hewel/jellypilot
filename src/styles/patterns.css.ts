import { style } from '@vanilla-extract/css';

import { sprinkles } from './sprinkles.css';
import { vars } from './vars.css';

export const srOnly = style({
  border: 0,
  clip: 'rect(0 0 0 0)',
  height: '1px',
  margin: '-1px',
  overflow: 'hidden',
  padding: 0,
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: '1px',
});

export const fullWidth = sprinkles({
  width: 'full',
});

export const icon3_5 = style({
  height: vars.space['3_5'],
  width: vars.space['3_5'],
});

export const icon4 = sprinkles({
  height: '4',
  width: '4',
});

export const icon4_5 = style({
  height: '1.125rem',
  width: '1.125rem',
});

export const icon5 = sprinkles({
  height: '5',
  width: '5',
});

export const icon6 = sprinkles({
  height: '6',
  width: '6',
});

export const spinner = style({
  animation: 'spin 1s linear infinite',
});

export const currentFill = style({
  fill: 'currentColor',
});

export const pill = style({
  borderRadius: vars.borderRadius.full,
});

export const mono = style({
  fontFamily: vars.font.mono,
});
