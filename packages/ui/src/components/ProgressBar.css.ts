import { style } from '@vanilla-extract/css'

export const progressRoot = style({
  width: '100%',
  height: '0.5rem',
  borderRadius: '9999px',
  background: 'var(--colors-muted, #f4f4f5)',
  overflow: 'hidden',
})

export const progressFill = style({
  height: '100%',
  background: 'var(--colors-primary, #18181b)',
})
