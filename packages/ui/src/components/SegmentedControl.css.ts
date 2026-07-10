import { style } from '@vanilla-extract/css'

export const segmentedRoot = style({
  display: 'inline-flex',
  gap: '0.25rem',
  padding: '0.25rem',
  borderRadius: '0.5rem',
  background: 'var(--colors-muted, #f4f4f5)',
  selectors: {
    '&[data-layout="fill"]': {
      display: 'flex',
      width: '100%',
    },
    '&[data-orientation="vertical"]': {
      flexDirection: 'column',
    },
  },
})

export const segmentedItem = style({
  flex: '1 1 auto',
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: '0.375rem',
  minHeight: '2rem',
  paddingInline: '0.75rem',
  border: 0,
  borderRadius: '0.375rem',
  background: 'transparent',
  cursor: 'pointer',
  fontWeight: 600,
  selectors: {
    '&[data-selected="true"]': {
      background: 'var(--colors-background, #fff)',
      boxShadow: '0 1px 2px rgb(0 0 0 / 0.08)',
    },
    '&:disabled': {
      opacity: 0.5,
      cursor: 'not-allowed',
    },
  },
})
