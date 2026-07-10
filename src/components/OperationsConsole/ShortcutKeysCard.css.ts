import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

const color = {
  onSurfaceVariant: 'var(--jellypilot-color-on-surface-variant)',
  secondary: 'var(--jellypilot-color-secondary)',
};

export const field = style([
  atomic({
    display: 'block',
  }),
]);

export const input = style([
  atomic({
    fontWeight: 'semibold',
    width: 'full',
  }),
  {
    color: color.secondary,
    fontFamily: 'var(--jellypilot-font-mono)',
  },
]);

export const description = style([
  {
    color: color.onSurfaceVariant,
    fontSize: '12px',
    lineHeight: '16px',
  },
]);
