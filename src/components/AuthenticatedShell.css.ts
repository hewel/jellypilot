import { projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';

export const main = style({
  color: projectTheme.color.onSurface,
  display: 'flex',
  flexDirection: 'column',
  marginInline: 'auto',
  width: '100%',
  // Reserve space for Now Playing + Theme + Settings cluster.
  paddingBottom: '11rem',
});

export const floatingControls = style({
  alignItems: 'center',
  borderRadius: projectTheme.borderRadius['3xl'],
  boxShadow: projectTheme.shadow.lg,
  display: 'flex',
  flexDirection: 'column',
  gap: projectTheme.space['2'],
  padding: projectTheme.space['1'],
  position: 'fixed',
  zIndex: projectTheme.zIndex['100'],
  background: projectTheme.color.surfaceContainerLow,
  border: `1px solid ${projectTheme.color.outlineVariant}`,
  bottom: `max(${projectTheme.space['4']}, env(safe-area-inset-bottom))`,
  right: `max(${projectTheme.space['4']}, env(safe-area-inset-right))`,
});
