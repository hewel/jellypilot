import { style } from '@vanilla-extract/css';

import { vars } from '../styles/vars.css';

export const control = style({
  width: '2.75rem',
  height: '2.75rem',
  minWidth: '44px',
  minHeight: '44px',
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: vars.borderRadius.full,
  border: `1px solid ${vars.color.outlineVariant}`,
  background: vars.color.surfaceContainer,
  color: vars.color.onSurface,
  cursor: 'pointer',
  selectors: {
    '&:focus-visible': {
      outline: `2px solid ${vars.color.primary}`,
      outlineOffset: '2px',
    },
  },
});

export const icon = style({
  width: '1.25rem',
  height: '1.25rem',
});
