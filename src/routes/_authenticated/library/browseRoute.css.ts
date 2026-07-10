import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

import { LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS } from '../../../utils/libraryBrowseLayout';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const root = style({
  minWidth: 0,
});

export const section = atomic({
  display: 'grid',
  gap: 4,
});

export const header = style([
  atomic({
    display: 'flex',
    gap: 2,
  }),
  {
    flexDirection: 'column',
    '@media': {
      'screen and (min-width: 640px)': {
        alignItems: 'center',
        flexDirection: 'row',
        justifyContent: 'space-between',
      },
    },
  },
]);

export const title = style({
  color: 'var(--jellypilot-color-on-surface)',
  fontSize: 'var(--jellypilot-font-size-22)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  lineHeight: 'var(--jellypilot-line-height-28)',
});

export const count = style({
  color: mix('var(--jellypilot-color-on-surface-variant)', 0.8),
  fontSize: 'var(--jellypilot-font-size-12)',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: 'var(--jellypilot-line-height-16)',
});

export const grid = style([
  atomic({
    display: 'grid',
    gap: 3,
  }),
  {
    gridTemplateColumns: LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS,
  },
]);

export const fade = style({
  animation: 'fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards',
});

export const virtualGrid = style({
  animation: 'fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards',
});

export const virtualCanvas = style({
  position: 'relative',
  width: '100%',
});

export const virtualRow = style({
  left: 0,
  position: 'absolute',
  top: 0,
  width: '100%',
});

export const loadMoreError = style([
  atomic({
    display: 'flex',
    gap: 3,
  }),
  {
    flexDirection: 'column',
    paddingTop: 'var(--jellypilot-space-2)',
    alignItems: 'center',
  },
]);

export const error = style({
  color: 'var(--jellypilot-color-error)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  textAlign: 'center',
});

export const pillButton = style({
  borderRadius: 'var(--jellypilot-border-radius-full)',
});

export const icon4 = style({
  height: 'var(--jellypilot-space-4)',
  width: 'var(--jellypilot-space-4)',
});

export const spin = style({
  animation: 'spin 1s linear infinite',
});

export const sentinel = style({
  height: '1px',
  width: '100%',
});

export const menuTrigger = style([
  atomic({
    items: 'center',
    justify: 'center',
    display: 'flex',
  }),
  {
    alignItems: 'center',
    border: 'none',
    borderLeft: '1px solid var(--jellypilot-color-outline-variant)',
    color: 'var(--jellypilot-color-on-surface)',
    flex: 'none',
    height: 'var(--jellypilot-space-10)',
    outline: 'none',
    paddingInline: 'var(--jellypilot-space-2)',
    textAlign: 'left',
    transitionDuration: 'var(--jellypilot-duration-200)',
    transitionProperty: 'color',
    width: 'var(--jellypilot-space-10)',
    selectors: {
      '&:hover': {
        color: 'var(--jellypilot-color-secondary)',
      },
      '&:disabled': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
      '&[data-state="on"]': {
        background: mix('var(--jellypilot-color-secondary-container)', 0.45),
        color: 'var(--jellypilot-color-on-secondary-container)',
      },
    },
    '@media': {
      'screen and (min-width: 640px)': {
        height: 'var(--jellypilot-space-12)',
        width: 'var(--jellypilot-space-12)',
      },
    },
  },
]);

export const menuContent = style({
  background: 'var(--jellypilot-color-surface-container-lowest)',
  border: `1px solid var(--jellypilot-color-outline-variant)`,
  borderRadius: 'var(--jellypilot-radius-lg)',
  boxShadow: 'var(--jellypilot-shadow-2xl)',
  maxHeight: '15rem',
  minWidth: '12rem',
  outline: 'none',
  overflowY: 'auto',
  padding: 'var(--jellypilot-space-2)',
  zIndex: 50,
});

export const menuLabel = style({
  fontSize: 'var(--jellypilot-font-size-12)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  padding: 'var(--jellypilot-space-2) var(--jellypilot-space-3_5)',
});

export const menuItem = style([
  atomic({
    display: 'flex',
    items: 'center',
    justify: 'between',
    rounded: 'xl',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    cursor: 'pointer',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    padding: 'var(--jellypilot-space-2_5) var(--jellypilot-space-3_5)',
    transitionProperty: 'background-color, color',
    border: 0,
    selectors: {
      '&:hover': {
        background: 'var(--jellypilot-color-surface-container-high)',
        color: 'var(--jellypilot-color-on-surface)',
      },
      '&[data-disabled]': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const menuText = style({
  fontWeight: 'var(--jellypilot-font-weight-medium)',
});

export const menuCheck = style({
  color: 'var(--jellypilot-color-secondary)',
  height: 'var(--jellypilot-space-4)',
  width: 'var(--jellypilot-space-4)',
});

export const separator = style({
  borderTop: `1px solid ${mix('var(--jellypilot-color-outline-variant)', 0.6)}`,
  marginBlock: 'var(--jellypilot-space-1)',
});

export const controlsNav = style({
  alignItems: 'flex-end',
  display: 'flex',
  flexDirection: 'row',
  justifyContent: 'space-between',
});

export const controlGroup = style([
  atomic({
    display: 'flex',
    wrap: 'wrap',
    justify: 'flex-end',
  }),
  {
    borderRadius: 'var(--jellypilot-radius-2xl)',
    minWidth: 0,
    overflow: 'hidden',
  },
]);

export const skeletonTitle = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix('var(--jellypilot-color-surface-container-high)', 0.7),
  borderRadius: 'var(--jellypilot-radius-md)',
  height: 'var(--jellypilot-space-7)',
  width: '8rem',
});

export const skeletonCount = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix('var(--jellypilot-color-surface-container-high)', 0.6),
  borderRadius: 'var(--jellypilot-radius-md)',
  height: 'var(--jellypilot-space-4)',
  width: 'var(--jellypilot-space-24)',
});
