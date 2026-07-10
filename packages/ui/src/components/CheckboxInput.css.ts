import { style } from '@vanilla-extract/css'

export const checkboxRoot = style({
  gap: '0.25rem',
})

export const checkboxLabel = style({
  display: 'inline-flex',
  alignItems: 'center',
  gap: '0.5rem',
  fontSize: '0.875rem',
  cursor: 'pointer',
  selectors: {
    [`${checkboxRoot}:has(input:disabled) &`]: {
      opacity: 0.5,
      cursor: 'not-allowed',
    },
  },
})
