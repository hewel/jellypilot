import { style } from '@vanilla-extract/css';

import { vars } from '../styles/vars.css';

export const trigger = style({
  height: '2.75rem',
  minHeight: '44px',
  minWidth: '44px',
  padding: '0 !important',
  width: '2.75rem',
});

export const triggerIcon = style({
  height: vars.space['5'],
  width: vars.space['5'],
});

export const content = style({
  background: vars.color.surfaceContainerLow,
  border: `1px solid ${vars.color.outlineVariant}`,
  display: 'flex',
  flexDirection: 'column',
  maxHeight: 'min(90vh, 56rem)',
  outline: 'none',
  overflow: 'auto',
  width: 'min(48rem, calc(100vw - 2rem))',
});

export const appearanceSection = style({
  borderBottom: `1px solid ${vars.color.outlineVariant}`,
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['3'],
  marginBottom: vars.space['4'],
  paddingBottom: vars.space['4'],
  width: '100%',
});

export const appearanceTitle = style({
  color: vars.color.onSurface,
  fontSize: vars.fontSize['16'],
  fontWeight: vars.fontWeight.bold,
  margin: 0,
});

export const appearanceDescription = style({
  color: vars.color.onSurfaceVariant,
  fontSize: vars.fontSize['12'],
  margin: 0,
});

export const appearanceOptions = style({
  display: 'grid',
  gap: vars.space['2'],
  gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
  width: '100%',
});

export const appearanceOption = style({
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.lg,
  background: vars.color.surfaceContainer,
  color: vars.color.onSurface,
  cursor: 'pointer',
  minHeight: '2.75rem',
  selectors: {
    '&[data-selected="true"]': {
      borderColor: vars.color.primary,
      boxShadow: `inset 0 0 0 1px ${vars.color.primary}`,
    },
  },
});

export const body = style({
  flex: 1,
  minHeight: 0,
  overflowY: 'auto',
});

export const hiddenClose = style({
  position: 'absolute',
  width: '1px',
  height: '1px',
  overflow: 'hidden',
  clip: 'rect(0 0 0 0)',
});
