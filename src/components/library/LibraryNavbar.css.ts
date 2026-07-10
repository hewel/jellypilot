import { atomic } from '@jellypilot/atomic-css';
import { globalStyle, style } from '@vanilla-extract/css';

export const root = style([
  atomic({
    position: 'sticky',
    rounded: '2xl',
    z: '100',
  }),
  {
    backdropFilter: 'blur(12px)',
    boxShadow: 'var(--jellypilot-shadow-xl)',
    background:
      'color-mix(in srgb, var(--jellypilot-color-surface-container-low) 75%, transparent)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    top: 'var(--jellypilot-space-2)',
  },
]);

export const inner = style([
  atomic({
    display: 'flex',
    items: 'center',
    justify: 'between',
    wrap: 'wrap',
  }),
  {
    flexDirection: 'row',
    gap: 'var(--jellypilot-space-2)',
    '@media': {
      'screen and (min-width: 640px)': {
        gap: 'var(--jellypilot-space-4)',
      },
    },
  },
]);

export const segments = style([
  atomic({
    position: 'relative',
    display: 'flex',
    rounded: 'xl',
  }),
  {
    gap: 'var(--jellypilot-space-1)',
    minWidth: '0',
    flexWrap: 'wrap',
    padding: 'var(--jellypilot-space-1)',
  },
]);

globalStyle(`${segments} [data-part="item"]`, {
  position: 'relative',
  display: 'inline-flex',
  minHeight: 'var(--jellypilot-space-10)',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: 'var(--jellypilot-radius-lg)',
  paddingInline: 'var(--jellypilot-space-4)',
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 'var(--jellypilot-line-height-20)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  cursor: 'pointer',
  transitionProperty: 'background-color, color',
});

globalStyle(`${segments} [data-part="item"]:hover`, {
  color: 'var(--jellypilot-color-on-surface)',
});

globalStyle(`${segments} [data-part="item"][data-selected="true"]`, {
  color: 'var(--jellypilot-color-on-secondary-container)',
  background: 'var(--jellypilot-color-surface-container-low)',
  boxShadow: 'var(--jellypilot-shadow-sm)',
});

globalStyle(`${segments} [data-part="item"][data-disabled="true"]`, {
  cursor: 'not-allowed',
  opacity: 0.5,
});

globalStyle(`${segments} [data-part="item"]:focus-visible`, {
  outline: '2px solid var(--jellypilot-color-secondary)',
  outlineOffset: '2px',
});

globalStyle(`${segments} [data-part="item"][data-selected="true"]:focus-visible`, {
  outline: '2px solid var(--jellypilot-color-secondary)',
});

export const indicator = style({});

export const item = style({
  selectors: {
    '&[data-state="checked"]': {
      color: 'var(--jellypilot-color-on-secondary-container)',
    },
  },
});

export const homeIcon = style({
  height: '1.125rem',
  width: '1.125rem',
});

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

export const portalTarget = style([
  atomic({
    display: 'flex',
    minWidth: '0',
    flexGrow: '1',
    justify: 'flex-end',
  }),
]);
