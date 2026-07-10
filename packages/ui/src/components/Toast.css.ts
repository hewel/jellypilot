import { style } from '@vanilla-extract/css'

export const toastViewport = style({
  position: 'fixed',
  right: '1rem',
  bottom: '1rem',
  zIndex: 60,
  display: 'flex',
  flexDirection: 'column',
  gap: '0.5rem',
  width: 'min(22rem, calc(100vw - 2rem))',
})

export const toastItem = style({
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  padding: '0.75rem',
  boxShadow: '0 8px 24px rgb(0 0 0 / 0.14)',
})
