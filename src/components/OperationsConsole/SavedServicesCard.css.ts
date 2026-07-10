import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

const color = {
  error: 'var(--jellypilot-color-error)',
  onSurface: 'var(--jellypilot-color-on-surface)',
  onSurfaceVariant: 'var(--jellypilot-color-on-surface-variant)',
  onSecondaryContainer: 'var(--jellypilot-color-on-secondary-container)',
  outlineVariant: 'var(--jellypilot-color-outline-variant)',
  secondary: 'var(--jellypilot-color-secondary)',
  surfaceContainerHigh: 'var(--jellypilot-color-surface-container-high)',
  warning: 'var(--jellypilot-color-warning)',
  warningContainer: 'var(--jellypilot-color-warning-container)',
};

export const stack = style([
  atomic({
    display: 'grid',
    gap: 3,
  }),
]);

export const profile = style([
  atomic({
    borderRadius: '2xl',
    p: 4,
  }),
  {
    backgroundColor: color.surfaceContainerHigh,
    border: `1px solid ${color.outlineVariant}`,
  },
]);

export const activeProfile = style([
  atomic({
    display: 'block',
  }),
  {
    borderColor: color.secondary,
  },
]);

export const warningProfile = style([
  atomic({
    display: 'block',
  }),
  {
    borderColor: color.warningContainer,
  },
]);

export const profileInner = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 3,
  }),
  {
    '@media': {
      'screen and (min-width: 640px)': {
        alignItems: 'flex-start',
        flexDirection: 'row',
        justifyContent: 'space-between',
      },
    },
  },
]);

export const copy = style([
  atomic({
    minWidth: 0,
  }),
]);

export const titleRow = style([
  atomic({
    display: 'flex',
    alignItems: 'center',
    flexWrap: 'wrap',
    gap: 2,
  }),
]);

export const name = style([
  atomic({
    fontWeight: 'bold',
  }),
  {
    color: color.onSurface,
    fontSize: '15px',
    lineHeight: '22px',
  },
]);

export const pill = style([
  atomic({
    borderRadius: 'full',
    fontWeight: 'bold',
    px: 2,
    py: 0.5,
  }),
  {
    border: `1px solid ${color.outlineVariant}`,
    color: color.onSurfaceVariant,
    fontSize: '10px',
    lineHeight: '14px',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const activePill = style([
  atomic({
    display: 'inline-flex',
  }),
  {
    background: color.secondary,
    borderColor: 'transparent',
    color: color.onSecondaryContainer,
  },
]);

export const url = style([
  atomic({
    marginTop: 1,
  }),
  {
    color: color.secondary,
    fontSize: '12px',
    lineHeight: '16px',
    fontFamily: 'var(--jellypilot-font-mono)',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const user = style([
  atomic({
    alignItems: 'center',
    display: 'flex',
    gap: 1.5,
  }),
  {
    color: color.onSurfaceVariant,
    fontSize: '12px',
    lineHeight: '16px',
    marginTop: 1,
  },
]);

export const warning = style([
  atomic({
    alignItems: 'flex-start',
    display: 'flex',
    gap: 1.5,
    fontWeight: 'semibold',
  }),
  {
    color: color.warning,
    fontSize: '12px',
    lineHeight: '16px',
    marginTop: 2,
  },
]);

export const warningIcon = style([
  atomic({
    flexShrink: 0,
    marginTop: 0.5,
    width: 3.5,
    height: 3.5,
  }),
]);

export const actions = style([
  atomic({
    display: 'flex',
    flexShrink: 0,
    flexWrap: 'wrap',
    gap: 2,
  }),
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
        borderColor: color.error,
      },
    },
  },
]);

export const footer = style([
  atomic({
    marginTop: 5,
  }),
]);
