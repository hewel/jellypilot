import { projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';

export const control = style({
  width: '2.75rem',
  height: '2.75rem',
  minWidth: '44px',
  minHeight: '44px',
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: projectTheme.borderRadius.full,
  border: `1px solid ${projectTheme.color.outlineVariant}`,
  background: projectTheme.color.surfaceContainer,
  color: projectTheme.color.onSurface,
  cursor: 'pointer',
  selectors: {
    '&:focus-visible': {
      outline: `2px solid ${projectTheme.color.primary}`,
      outlineOffset: '2px',
    },
  },
});

export const icon = style({
  width: '1.25rem',
  height: '1.25rem',
});
