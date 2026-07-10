import { style } from '@vanilla-extract/css'

export const dialogRoot = style({
  position: 'fixed',
  inset: 0,
  zIndex: 50,
  display: 'grid',
  placeItems: 'center',
})

export const dialogBackdrop = style({
  position: 'absolute',
  inset: 0,
  background: 'rgb(0 0 0 / 0.45)',
})

export const dialogContent = style({
  position: 'relative',
  zIndex: 1,
  width: 'min(32rem, calc(100vw - 2rem))',
  maxHeight: 'min(80vh, 40rem)',
  overflow: 'auto',
  borderRadius: '0.75rem',
  background: 'var(--colors-background, #fff)',
  color: 'var(--colors-foreground, #0a0a0a)',
  padding: '1rem',
  boxShadow: '0 10px 30px rgb(0 0 0 / 0.2)',
})
