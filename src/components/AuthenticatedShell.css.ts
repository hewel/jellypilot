import { style } from '@vanilla-extract/css';

import { vars } from '../styles/vars.css';

export const main = style({
  color: vars.color.onSurface,
  display: 'flex',
  flexDirection: 'column',
  marginInline: 'auto',
  width: '100%',
  // Reserve space for Now Playing + Theme + Settings cluster.
  paddingBottom: '11rem',
});

export const floatingControls = style({
  alignItems: 'center',
  borderRadius: vars.borderRadius['3xl'],
  boxShadow: vars.shadow.lg,
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['2'],
  padding: vars.space['1'],
  position: 'fixed',
  zIndex: vars.zIndex['100'],
  background: vars.color.surfaceContainerLow,
  border: `1px solid ${vars.color.outlineVariant}`,
  bottom: `max(${vars.space['4']}, env(safe-area-inset-bottom))`,
  right: `max(${vars.space['4']}, env(safe-area-inset-right))`,
});
