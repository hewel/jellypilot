import { style } from '@vanilla-extract/css'

export const hoverCardRoot = style({
  position: 'relative',
  display: 'inline-flex',
})

export const hoverCardContent = style({
  position: 'absolute',
  zIndex: 40,
  top: 'calc(100% + 0.35rem)',
  left: 0,
  minWidth: '12rem',
  maxWidth: '20rem',
  padding: '0.75rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  boxShadow: '0 8px 20px rgb(0 0 0 / 0.12)',
})
