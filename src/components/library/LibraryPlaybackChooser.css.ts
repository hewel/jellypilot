import { atomic } from '@jellypilot/atomic-css';
import { globalStyle, style } from '@vanilla-extract/css';

export const content = style([
  atomic({
    position: 'relative',
    width: 'full',
  }),
  {
    background: 'transparent',
    display: 'grid',
    maxHeight: 'min(80vh, 40rem)',
    maxWidth: '42rem',
    outline: 'none',
    overflow: 'auto',
    padding: '0',
  },
]);
globalStyle(`${content} [data-part="close"]`, {
  display: 'none',
});

export const card = style([
  atomic({
    display: 'grid',
    gap: 4,
  }),
  {
    background: 'var(--jellypilot-color-surface-container)',
    border: '1px solid var(--jellypilot-color-outline-variant)',
    borderRadius: 'var(--jellypilot-radius-lg)',
    padding: '1rem',
  },
]);

export const eyebrow = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-11)',
  letterSpacing: '0.08em',
  lineHeight: 'var(--jellypilot-line-height-16)',
  textTransform: 'uppercase',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
});

export const fields = style({
  display: 'grid',
  gap: 'var(--jellypilot-space-4)',
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
    },
  },
});

export const actions = style([
  atomic({
    display: 'flex',
    flexWrap: 'wrap',
    justifyContent: 'flex-end',
    gap: 3,
  }),
]);

export const icon = style({
  width: '1rem',
  height: '1rem',
});

export const pillButton = style({
  borderRadius: 'var(--jellypilot-border-radius-full)',
});

export const playIcon = style([
  atomic({
    width: 4,
    height: 4,
  }),
  {
    fill: 'currentColor',
  },
]);
