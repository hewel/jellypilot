import { style } from '@vanilla-extract/css'

export const switchRoot = style({
  gap: '0.25rem',
})

export const switchControl = style({
  position: 'relative',
  width: '2.5rem',
  height: '1.5rem',
  borderRadius: '9999px',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-muted, #f4f4f5)',
  padding: 0,
  cursor: 'pointer',
  verticalAlign: 'middle',
  selectors: {
    '&[aria-checked="true"]': {
      background: 'var(--colors-primary, #18181b)',
    },
    '&:disabled': {
      opacity: 0.5,
      cursor: 'not-allowed',
    },
  },
})

export const switchThumb = style({
  position: 'absolute',
  top: '0.125rem',
  left: '0.125rem',
  width: '1.125rem',
  height: '1.125rem',
  borderRadius: '9999px',
  background: '#fff',
  transition: 'transform 120ms ease',
  selectors: {
    [`${switchControl}[aria-checked="true"] &`]: {
      transform: 'translateX(1rem)',
    },
  },
  '@media': {
    '(prefers-reduced-motion: reduce)': {
      transition: 'none',
    },
  },
})
