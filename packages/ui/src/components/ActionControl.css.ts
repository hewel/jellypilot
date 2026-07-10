import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const buttonBase = style([
  atomic({
    display: 'inline-flex',
    items: 'center',
    justify: 'center',
    gap: 2,
    rounded: 'md',
    fontWeight: 'semibold',
  }),
  {
    border: '1px solid transparent',
    cursor: 'pointer',
    selectors: {
      '&:disabled': {
        opacity: 0.5,
        cursor: 'not-allowed',
      },
      '&[data-loading="true"]': {
        cursor: 'progress',
      },
    },
  },
]);

export const buttonPrimary = style({
  background: 'var(--colors-primary, #18181b)',
  color: 'var(--colors-primary-foreground, #fafafa)',
});

export const buttonSecondary = style({
  background: 'var(--colors-muted, #f4f4f5)',
  color: 'var(--colors-foreground, #0a0a0a)',
});

export const buttonOutline = style({
  background: 'transparent',
  borderColor: 'var(--colors-border, #e4e4e7)',
  color: 'var(--colors-foreground, #0a0a0a)',
});

export const buttonGhost = style({
  background: 'transparent',
  color: 'var(--colors-foreground, #0a0a0a)',
});

export const buttonSm = style({
  minHeight: '2rem',
  paddingInline: '0.75rem',
  fontSize: '0.875rem',
});

export const buttonMd = style({
  minHeight: '2.5rem',
  paddingInline: '1rem',
  fontSize: '1rem',
});

export const buttonLg = style({
  minHeight: '2.75rem',
  paddingInline: '1.25rem',
  fontSize: '1.125rem',
});

export const iconButton = style({
  minWidth: '2.5rem',
  minHeight: '2.5rem',
  padding: 0,
});

export const togglePressed = style({
  outline: '2px solid #2563eb',
  outlineOffset: '1px',
});
