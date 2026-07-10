import { style } from '@vanilla-extract/css'

export const spinnerStyle = style({
  display: 'inline-block',
  width: '1.25rem',
  height: '1.25rem',
  borderRadius: '9999px',
  border: '2px solid var(--colors-border, #e4e4e7)',
  borderTopColor: 'var(--colors-primary, #18181b)',
  animation: 'jp-spin 0.8s linear infinite',
  '@media': {
    '(prefers-reduced-motion: reduce)': {
      animation: 'none',
      borderTopColor: 'var(--colors-primary, #18181b)',
    },
  },
})
