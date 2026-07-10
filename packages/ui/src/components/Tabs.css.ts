import { style } from '@vanilla-extract/css'

export const tabRoot = style({
  display: 'flex',
  flexDirection: 'column',
  gap: '0.75rem',
  selectors: {
    '&[data-orientation="vertical"]': {
      flexDirection: 'row',
      alignItems: 'flex-start',
    },
  },
})

export const tabList = style({
  display: 'flex',
  gap: '0.25rem',
  borderBottom: '1px solid var(--colors-border, #e4e4e7)',
  selectors: {
    [`${tabRoot}[data-orientation="vertical"] &`]: {
      flexDirection: 'column',
      borderBottom: 0,
      borderRight: '1px solid var(--colors-border, #e4e4e7)',
    },
  },
})

export const tabTrigger = style({
  appearance: 'none',
  border: 0,
  background: 'transparent',
  padding: '0.5rem 0.75rem',
  cursor: 'pointer',
  fontWeight: 600,
  color: 'var(--colors-muted-foreground, #71717a)',
  selectors: {
    '&[data-selected="true"]': {
      color: 'var(--colors-foreground, #0a0a0a)',
      boxShadow: 'inset 0 -2px 0 var(--colors-primary, #18181b)',
    },
    '&:disabled': {
      opacity: 0.5,
      cursor: 'not-allowed',
    },
  },
})

export const tabPanel = style({
  minWidth: 0,
})
