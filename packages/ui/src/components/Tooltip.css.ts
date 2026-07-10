import { style } from '@vanilla-extract/css'

export const tooltipRoot = style({
  position: 'relative',
  display: 'inline-flex',
})

export const tooltipContent = style({
  position: 'absolute',
  zIndex: 40,
  bottom: 'calc(100% + 0.35rem)',
  left: '50%',
  transform: 'translateX(-50%)',
  maxWidth: '16rem',
  padding: '0.35rem 0.5rem',
  borderRadius: '0.375rem',
  background: '#18181b',
  color: '#fafafa',
  fontSize: '0.75rem',
  whiteSpace: 'nowrap',
})
