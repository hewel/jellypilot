import { projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';

export const trigger = style({
  height: '2.75rem',
  minHeight: '44px',
  minWidth: '44px',
  padding: '0 !important',
  width: '2.75rem',
});

export const triggerIcon = style({
  height: projectTheme.space['5'],
  width: projectTheme.space['5'],
});

export const content = style({
  background: projectTheme.color.surfaceContainerLow,
  border: `1px solid ${projectTheme.color.outlineVariant}`,
  display: 'flex',
  flexDirection: 'column',
  maxHeight: 'min(90vh, 56rem)',
  outline: 'none',
  overflow: 'auto',
  width: 'min(48rem, calc(100vw - 2rem))',
});

export const appearanceSection = style({
  borderBottom: `1px solid ${projectTheme.color.outlineVariant}`,
  display: 'flex',
  flexDirection: 'column',
  gap: projectTheme.space['3'],
  marginBottom: projectTheme.space['4'],
  paddingBottom: projectTheme.space['4'],
  width: '100%',
});

export const appearanceTitle = style({
  color: projectTheme.color.onSurface,
  fontSize: projectTheme.fontSize['16'],
  fontWeight: projectTheme.fontWeight.bold,
  margin: 0,
});

export const appearanceDescription = style({
  color: projectTheme.color.onSurfaceVariant,
  fontSize: projectTheme.fontSize['12'],
  margin: 0,
});

export const appearanceOptions = style({
  display: 'grid',
  gap: projectTheme.space['2'],
  gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
  width: '100%',
});

export const appearanceOption = style({
  border: `1px solid ${projectTheme.color.outlineVariant}`,
  borderRadius: projectTheme.borderRadius.lg,
  background: projectTheme.color.surfaceContainer,
  color: projectTheme.color.onSurface,
  cursor: 'pointer',
  minHeight: '2.75rem',
  selectors: {
    '&[data-selected="true"]': {
      borderColor: projectTheme.color.primary,
      boxShadow: `inset 0 0 0 1px ${projectTheme.color.primary}`,
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
