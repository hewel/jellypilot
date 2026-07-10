import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const stack = style([
  atomic({
    display: 'grid',
  }),
  {
    gap: 'var(--jellypilot-space-6)',
  },
]);

export const backdrop = style([
  atomic({
    inset: 0,
    position: 'fixed',
    z: 60,
  }),
  {
    background: 'var(--jellypilot-color-background)',
    transitionDuration: 'var(--jellypilot-duration-300)',
    transitionProperty: 'opacity',
    selectors: {
      '&[data-state="closed"]': { opacity: 0 },
      '&[data-state="open"]': { opacity: 1 },
    },
  },
]);

export const positioner = style([
  atomic({
    alignItems: 'center',
    display: 'flex',
    justify: 'center',
    overflowY: 'auto',
    position: 'fixed',
    z: 60,
  }),
  {
    padding: 'var(--jellypilot-space-4)',
  },
]);

export const positionerFill = style({
  inset: 0,
});

export const content = style([
  atomic({
    position: 'relative',
    width: 'full',
  }),
  {
    maxWidth: '48rem',
    outline: 'none',
  },
]);

export const closeButton = style([
  atomic({
    position: 'absolute',
    rounded: 'xl',
  }),
  {
    right: 'var(--jellypilot-space-4)',
    top: 'var(--jellypilot-space-4)',
    background: 'var(--jellypilot-color-surface-container-high)',
    border: '1px solid var(--jellypilot-color-outline-variant)',
    color: 'var(--jellypilot-color-on-surface-variant)',
    cursor: 'pointer',
    zIndex: 10,
    selectors: {
      '&:hover': {
        borderColor: 'var(--jellypilot-color-secondary)',
        color: 'var(--jellypilot-color-secondary)',
      },
      '&:focus-visible': {
        outline: 'none',
      },
    },
  },
]);
