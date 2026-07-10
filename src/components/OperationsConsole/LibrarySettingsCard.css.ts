import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const toggle = style([
  atomic({
    display: 'flex',
    items: 'flex-start',
    gap: 3,
    rounded: '2xl',
    p: 4,
    textAlign: 'left',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    cursor: 'pointer',
    selectors: {
      '&:focus-visible': {
        outline: '2px solid var(--jellypilot-color-primary)',
        outlineOffset: '2px',
      },
    },
  },
]);

export const checkbox = style([
  atomic({
    display: 'inline-flex',
    items: 'center',
    justify: 'center',
    flexShrink: 0,
    rounded: 'lg',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline)`,
    color: 'var(--jellypilot-color-on-primary)',
    fontSize: '0.6875rem',
    lineHeight: 'var(--jellypilot-line-height-none)',
    height: '1.375rem',
    marginTop: 'var(--jellypilot-space-0_5)',
    transitionDuration: 'var(--jellypilot-duration-200)',
    transitionProperty: 'background-color, border-color',
    width: '1.375rem',
    selectors: {
      '&:hover': {
        borderColor: 'var(--jellypilot-color-primary)',
      },
    },
  },
]);

export const checkboxChecked = style({
  background: 'var(--jellypilot-color-primary)',
  borderColor: 'var(--jellypilot-color-primary)',
});

export const copy = style([
  atomic({
    minWidth: 0,
  }),
]);

export const title = style([
  atomic({
    display: 'block',
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
  },
]);

export const description = style([
  atomic({
    mt: 1,
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    opacity: 0.8,
  },
]);
