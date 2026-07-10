import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const root = atomic({
  py: '2rem',
  textAlign: 'center',
});

export const text = style([
  atomic({
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
  }),
  { opacity: 0.7 },
]);
