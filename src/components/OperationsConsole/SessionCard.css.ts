import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

const color = {
  error: 'var(--jellypilot-color-error)',
  onSurface: 'var(--jellypilot-color-on-surface)',
  onSurfaceVariant: 'var(--jellypilot-color-on-surface-variant)',
  outlineVariant: 'var(--jellypilot-color-outline-variant)',
  surfaceContainer: 'var(--jellypilot-color-surface-container)',
  surfaceContainerLow: 'var(--jellypilot-color-surface-container-low)',
};

export const card = style([
  atomic({
    display: 'block',
  }),
  {
    backgroundColor: color.surfaceContainer,
    border: `1px solid ${color.outlineVariant}`,
    selectors: {
      '&:hover': {
        borderColor: color.error,
      },
    },
  },
]);

export const header = style([
  atomic({
    display: 'flex',
    alignItems: 'flex-start',
    gap: 3,
  }),
]);

export const cardIcon = style([
  atomic({
    height: 5,
    marginTop: 1,
    width: 5,
  }),
  {
    color: color.error,
  },
]);

export const title = style([
  atomic({
    fontWeight: 'semibold',
  }),
  {
    color: color.onSurface,
    fontSize: '16px',
    lineHeight: '24px',
  },
]);

export const description = style([
  atomic({
    marginTop: 1,
  }),
  {
    color: color.onSurfaceVariant,
    fontSize: '12px',
    lineHeight: '16px',
  },
]);

export const signOutButton = style([
  atomic({
    marginTop: 5,
    width: 'full',
  }),
  {
    borderColor: color.error,
    color: color.error,
    selectors: {
      '&:hover': {
        background: 'color-mix(in srgb, var(--jellypilot-color-error) 12%, transparent)',
        borderColor: color.error,
      },
    },
  },
]);

export const backdrop = style([
  atomic({
    inset: 0,
    position: 'fixed',
    zIndex: 50,
  }),
  {
    backgroundColor: 'rgb(0 0 0 / 0.55)',
    transitionDuration: 'var(--jellypilot-duration-300)',
    transitionProperty: 'opacity',
    selectors: {
      '&[data-state="closed"]': {
        opacity: 0,
      },
      '&[data-state="open"]': {
        opacity: 1,
      },
    },
  },
]);

export const positioner = style([
  atomic({
    alignItems: 'center',
    display: 'flex',
    justifyContent: 'center',
    p: 4,
    position: 'fixed',
    zIndex: 50,
  }),
]);

export const positionerFill = style([
  atomic({
    inset: 0,
  }),
]);

export const content = style([
  atomic({
    overflow: 'hidden',
    p: 6,
    position: 'relative',
  }),
  {
    borderRadius: '2rem',
    backgroundColor: color.surfaceContainerLow,
    border: `1px solid ${color.outlineVariant}`,
    maxWidth: '28rem',
    transitionDuration: 'var(--jellypilot-duration-300)',
    transitionProperty: 'background-color, border-color, opacity, transform',
    selectors: {
      '&[data-state="closed"]': {
        opacity: 0,
        transform: 'translateY(0.25rem)',
      },
      '&[data-state="open"]': {
        opacity: 1,
        transform: 'translateY(0)',
      },
    },
  },
]);

export const dialogTitle = style([
  atomic({
    alignItems: 'center',
    display: 'flex',
    fontWeight: 'bold',
    gap: 2,
  }),
  {
    color: color.onSurface,
    fontSize: '22px',
    lineHeight: '28px',
  },
]);

export const dialogIcon = style([
  atomic({
    height: 6,
    width: 6,
  }),
  {
    color: color.error,
  },
]);

export const dialogDescription = style([
  atomic({
    marginTop: 3,
  }),
  {
    color: color.onSurfaceVariant,
    fontSize: '14px',
    lineHeight: '20px',
  },
]);

export const actions = style([
  atomic({
    display: 'flex',
    flexDirection: 'column-reverse',
    gap: 3,
    marginTop: 6,
  }),
  {
    '@media': {
      'screen and (min-width: 640px)': {
        flexDirection: 'row',
        justifyContent: 'flex-end',
      },
    },
  },
]);

export const dangerButton = style([
  atomic({
    display: 'inline-flex',
  }),
  {
    borderColor: color.error,
    color: color.error,
    selectors: {
      '&:hover': {
        background: 'color-mix(in srgb, var(--jellypilot-color-error) 12%, transparent)',
      },
    },
  },
]);
