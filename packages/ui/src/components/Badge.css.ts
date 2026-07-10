import { atomic } from '@jellypilot/atomic-css'
import { style } from '@vanilla-extract/css'

export const badgeStyle = style([
  atomic({
    display: 'inline-flex',
    items: 'center',
    justify: 'center',
    rounded: 'full',
    fontWeight: 'semibold',
  }),
  {
    minHeight: '1.25rem',
    paddingInline: '0.5rem',
    fontSize: '0.75rem',
    background: 'var(--colors-muted, #f4f4f5)',
    color: 'var(--colors-foreground, #0a0a0a)',
  },
])
