import { style } from '@vanilla-extract/css';

export const input = style({
  appearance: 'none',
  background: 'transparent',
  cursor: 'inherit',
  height: '100%',
  inset: 0,
  margin: 0,
  opacity: 0,
  position: 'absolute',
  width: '100%',
  zIndex: 2,
});

export const thumbPosition = style({
  display: 'flex',
  left: 0,
  pointerEvents: 'none',
  position: 'absolute',
  top: '50%',
  transform: 'translate(-50%, -50%)',
  zIndex: 1,
});
