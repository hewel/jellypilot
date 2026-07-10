import { atomic } from '@jellypilot/atomic-css';
import { projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';

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

export const fullWidth = atomic({
  width: '100%',
});

export const icon3_5 = style({
  height: projectTheme.space['3_5'],
  width: projectTheme.space['3_5'],
});

export const icon4 = atomic({
  height: '1rem',
  width: '1rem',
});

export const icon4_5 = style({
  height: '1.125rem',
  width: '1.125rem',
});

export const icon5 = atomic({
  height: '1.25rem',
  width: '1.25rem',
});

export const icon6 = atomic({
  height: '1.5rem',
  width: '1.5rem',
});

export const spinner = style({
  animation: 'spin 1s linear infinite',
});

export const currentFill = style({
  fill: 'currentColor',
});

export const pill = style({
  borderRadius: projectTheme.borderRadius.full,
});

export const mono = style({
  fontFamily: projectTheme.font.mono,
});
