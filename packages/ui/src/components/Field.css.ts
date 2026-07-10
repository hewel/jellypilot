import { atomic } from '@jellypilot/atomic-css'
import { style } from '@vanilla-extract/css'

export const fieldRoot = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 1,
  }),
  {},
])

export const fieldLabel = style({
  fontSize: '0.875rem',
  fontWeight: 600,
})

export const fieldControl = style([
  atomic({
    width: 'full',
    rounded: 'md',
    p: 2,
  }),
  {
    border: '1px solid var(--colors-border, #e4e4e7)',
    background: 'var(--colors-background, #fff)',
    color: 'var(--colors-foreground, #0a0a0a)',
    selectors: {
      '&:disabled': { opacity: 0.5, cursor: 'not-allowed' },
      '&[aria-invalid="true"]': { borderColor: '#dc2626' },
    },
  },
])

export const fieldDescription = style({
  fontSize: '0.75rem',
  color: 'var(--colors-muted-foreground, #71717a)',
})

export const fieldError = style({
  fontSize: '0.75rem',
  color: '#dc2626',
})
