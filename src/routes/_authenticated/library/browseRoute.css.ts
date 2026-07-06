import { style } from '@vanilla-extract/css';

import { LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS } from '../../../utils/libraryBrowseLayout';

import { vars } from '../../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const root = style({
  minWidth: 0,
});

export const section = style({
  display: 'grid',
  gap: vars.space['4'],
});

export const header = style({
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['2'],
  '@media': {
    'screen and (min-width: 640px)': {
      alignItems: 'center',
      flexDirection: 'row',
      justifyContent: 'space-between',
    },
  },
});

export const title = style({
  color: vars.color.onSurface,
  fontSize: vars.fontSize['22'],
  fontWeight: vars.fontWeight.bold,
  lineHeight: vars.lineHeight['28'],
});

export const count = style({
  color: mix(vars.color.onSurfaceVariant, 0.8),
  fontSize: vars.fontSize['12'],
  fontVariantNumeric: 'tabular-nums',
  lineHeight: vars.lineHeight['16'],
});

export const grid = style({
  display: 'grid',
  gap: vars.space['3'],
  gridTemplateColumns: LIBRARY_BROWSE_GRID_TEMPLATE_COLUMNS,
});

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

export const loadMoreError = style({
  alignItems: 'center',
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['3'],
  paddingTop: vars.space['2'],
});

export const error = style({
  color: vars.color.error,
  fontSize: vars.fontSize['12'],
  lineHeight: vars.lineHeight['16'],
  textAlign: 'center',
});

export const pillButton = style({
  borderRadius: vars.borderRadius.full,
});

export const icon4 = style({
  height: vars.space['4'],
  width: vars.space['4'],
});

export const spin = style({
  animation: 'spin 1s linear infinite',
});

export const sentinel = style({
  height: '1px',
  width: '100%',
});

export const menuTrigger = style({
  alignItems: 'center',
  border: 0,
  borderLeft: `1px solid ${vars.color.outlineVariant}`,
  color: vars.color.onSurface,
  display: 'flex',
  flex: 'none',
  height: vars.space['10'],
  justifyContent: 'center',
  outline: 'none',
  paddingInline: vars.space['2'],
  textAlign: 'left',
  transitionDuration: vars.duration['200'],
  transitionProperty: 'color',
  width: vars.space['10'],
  selectors: {
    '&:hover': {
      color: vars.color.secondary,
    },
    '&:disabled': {
      cursor: 'not-allowed',
      opacity: 0.5,
    },
    '&[data-state="on"]': {
      background: mix(vars.color.secondaryContainer, 0.45),
      color: vars.color.onSecondaryContainer,
    },
  },
  '@media': {
    'screen and (min-width: 640px)': {
      height: vars.space['12'],
      width: vars.space['12'],
    },
  },
});

export const menuContent = style({
  backdropFilter: 'blur(12px)',
  background: vars.color.surfaceContainerLowest,
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.lg,
  boxShadow: vars.shadow['2xl'],
  maxHeight: '15rem',
  minWidth: '12rem',
  outline: 'none',
  overflowY: 'auto',
  padding: vars.space['2'],
  zIndex: 50,
});

export const menuLabel = style({
  fontSize: vars.fontSize['12'],
  fontWeight: vars.fontWeight.bold,
  padding: `${vars.space['2']} ${vars.space['3_5']}`,
});

export const menuItem = style({
  alignItems: 'center',
  borderRadius: vars.borderRadius.xl,
  color: vars.color.onSurfaceVariant,
  cursor: 'pointer',
  display: 'flex',
  fontSize: vars.fontSize['14'],
  justifyContent: 'space-between',
  lineHeight: vars.lineHeight['20'],
  padding: `${vars.space['2_5']} ${vars.space['3_5']}`,
  transitionProperty: 'background-color, color',
  selectors: {
    '&:hover': {
      background: vars.color.surfaceContainerHigh,
      color: vars.color.onSurface,
    },
    '&[data-disabled]': {
      cursor: 'not-allowed',
      opacity: 0.5,
    },
  },
});

export const menuText = style({
  fontWeight: vars.fontWeight.medium,
});

export const menuCheck = style({
  color: vars.color.secondary,
  height: vars.space['4'],
  width: vars.space['4'],
});

export const separator = style({
  borderTop: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
  marginBlock: vars.space['1'],
});

export const controlsNav = style({
  alignItems: 'flex-end',
  display: 'flex',
  flexDirection: 'row',
  justifyContent: 'space-between',
});

export const controlGroup = style({
  borderRadius: vars.borderRadius['2xl'],
  display: 'flex',
  flexWrap: 'wrap',
  justifyContent: 'flex-end',
  minWidth: 0,
  overflow: 'hidden',
});

export const skeletonTitle = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.7),
  borderRadius: vars.borderRadius.md,
  height: vars.space['7'],
  width: '8rem',
});

export const skeletonCount = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.6),
  borderRadius: vars.borderRadius.md,
  height: vars.space['4'],
  width: vars.space['24'],
});
