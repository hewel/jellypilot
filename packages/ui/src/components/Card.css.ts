import { atomic } from '@jellypilot/atomic-css'
import { style } from '@vanilla-extract/css'

export const cardStyle = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 3,
    p: 4,
    rounded: 'lg',
  }),
  {
    background: 'var(--colors-muted, #f4f4f5)',
    border: '1px solid var(--colors-border, #e4e4e7)',
    selectors: {
      '&[data-variant="outlined"]': {
        background: 'transparent',
      },
    },
  },
])
