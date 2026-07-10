import { style } from '@vanilla-extract/css'

export const popoverRoot = style({
  position: 'relative',
  display: 'inline-block',
})

export const popoverContent = style({
  position: 'fixed',
  zIndex: 40,
  minWidth: '12rem',
  maxWidth: 'min(20rem, calc(100vw - 2rem))',
  padding: '0.75rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  boxShadow: '0 8px 24px rgb(0 0 0 / 0.12)',
})
