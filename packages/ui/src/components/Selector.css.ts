import { style } from '@vanilla-extract/css'

export const selectorRoot = style({
  position: 'relative',
  display: 'inline-block',
  minWidth: '10rem',
})

export const selectorTrigger = style({
  width: '100%',
  minHeight: '2.5rem',
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: '0.5rem',
  padding: '0.5rem 0.75rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  cursor: 'pointer',
})

export const selectorContent = style({
  position: 'absolute',
  zIndex: 30,
  top: 'calc(100% + 0.25rem)',
  left: 0,
  right: 0,
  display: 'flex',
  flexDirection: 'column',
  gap: '0.125rem',
  padding: '0.25rem',
  borderRadius: '0.5rem',
  border: '1px solid var(--colors-border, #e4e4e7)',
  background: 'var(--colors-background, #fff)',
  boxShadow: '0 8px 20px rgb(0 0 0 / 0.12)',
})
