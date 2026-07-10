import { atomic } from '@jellypilot/atomic-css'
import { style } from '@vanilla-extract/css'

export const shellStyle = style([
  atomic({
    display: 'flex',
    width: 'full',
    gap: 4,
    p: 4,
  }),
  {
    minHeight: '100vh',
    alignItems: 'flex-start',
    '@media': {
      '(max-width: 720px)': {
        flexDirection: 'column',
      },
    },
  },
])

export const pageStyle = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 4,
    p: 2,
  }),
  {
    flex: 1,
    minWidth: 0,
  },
])

export const sectionStyle = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 2,
    p: 0,
    m: 0,
  }),
  {
    minWidth: '14rem',
  },
])

export const searchStyle = style({
  width: '100%',
  minHeight: '2.5rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  padding: '0.5rem 0.75rem',
})
