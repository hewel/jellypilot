import { style } from '@vanilla-extract/css'

/** Clip to 1px — not an atomic utility in v1. */
export const visuallyHiddenStyle = style({
  position: 'absolute',
  width: '1px',
  height: '1px',
  padding: 0,
  margin: '-1px',
  overflow: 'hidden',
  clip: 'rect(0, 0, 0, 0)',
  whiteSpace: 'nowrap',
  border: 0,
})
