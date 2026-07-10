import { style } from '@vanilla-extract/css'

export const menuRoot = style({
  position: 'relative',
  display: 'inline-block',
})

export const menuTrigger = style({
  minHeight: '2.5rem',
  paddingInline: '0.75rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  cursor: 'pointer',
})

export const menuContent = style({
  position: 'absolute',
  zIndex: 30,
  top: 'calc(100% + 0.25rem)',
  left: 0,
  minWidth: '10rem',
  display: 'flex',
  flexDirection: 'column',
  padding: '0.25rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  boxShadow: '0 8px 20px rgb(0 0 0 / 0.12)',
})
