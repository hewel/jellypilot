import { style } from '@vanilla-extract/css'

export const sliderRoot = style({
  width: '100%',
  gap: '0.5rem',
})

export const sliderTrack = style({
  position: 'relative',
  height: '0.375rem',
  borderRadius: '9999px',
  background: 'var(--colors-muted, #f4f4f5)',
})

export const sliderFill = style({
  position: 'absolute',
  top: 0,
  bottom: 0,
  left: 0,
  borderRadius: '9999px',
  background: 'var(--colors-primary, #18181b)',
})

export const sliderThumb = style({
  position: 'absolute',
  top: '50%',
  width: '1rem',
  height: '1rem',
  marginLeft: '-0.5rem',
  marginTop: '-0.5rem',
  borderRadius: '9999px',
  border: '2px solid var(--colors-primary, #18181b)',
  background: '#fff',
  padding: 0,
  cursor: 'pointer',
  selectors: {
    '&:disabled': {
      opacity: 0.5,
      cursor: 'not-allowed',
    },
  },
})
