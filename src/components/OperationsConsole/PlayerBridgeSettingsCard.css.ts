import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const field = style({
  display: 'block',
});

export const error = style({
  marginTop: 'var(--jellypilot-space-1_5)',
  color: 'var(--jellypilot-color-error)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  fontWeight: 'var(--jellypilot-font-weight-semibold)',
});

export const helper = style([
  atomic({
    mt: 1.5,
  }),
  {
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    color: 'var(--jellypilot-color-on-surface-variant)',
    opacity: 0.8,
  },
]);

export const detectRow = style({
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--jellypilot-space-2_5)',
  '@media': {
    'screen and (min-width: 640px)': {
      flexDirection: 'row',
    },
  },
});

export const flexInput = style({
  flex: 1,
  minWidth: 0,
});

export const detectButton = style({
  minHeight: 'var(--jellypilot-space-14)',
  '@media': {
    'screen and (min-width: 640px)': {
      minHeight: 0,
    },
  },
});

export const advancedTrigger = style({
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  paddingInline: 0,
  selectors: {
    '&:hover': {
      color: 'var(--jellypilot-color-secondary)',
    },
  },
});

export const chevron = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  height: '1.125rem',
  transitionDuration: 'var(--jellypilot-duration-200)',
  transitionProperty: 'color, transform',
  width: '1.125rem',
});

export const chevronOpen = style({
  color: 'var(--jellypilot-color-secondary)',
  transform: 'rotate(180deg)',
});

export const advancedPanel = style([
  atomic({
    mt: 3,
    rounded: '2xl',
    p: 4,
  }),
  {
    background: 'var(--jellypilot-color-surface-container-low)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    boxSizing: 'border-box',
  },
]);

export const subheading = style([
  atomic({
    display: 'flex',
    items: 'center',
    gap: 2,
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    textAlign: 'left',
  },
]);

export const subheadingAccent = style({
  background: 'var(--jellypilot-color-secondary)',
  borderRadius: 'var(--jellypilot-radius-md)',
  height: 'var(--jellypilot-space-3)',
  width: 'var(--jellypilot-space-1)',
});

export const textarea = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontFamily: 'var(--jellypilot-font-mono)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  height: 'auto',
  paddingBlock: 'var(--jellypilot-space-3_5)',
  width: '100%',
});

export const languagePanel = style([
  atomic({
    position: 'relative',
    rounded: '2xl',
    p: 5,
  }),
  {
    background: 'var(--jellypilot-color-surface-container-low)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
  },
]);

export const panelHeader = style({
  alignItems: 'flex-start',
  display: 'flex',
  flexWrap: 'wrap',
  gap: 'var(--jellypilot-space-4)',
  justifyContent: 'space-between',
});

export const languageTitle = style([
  atomic({
    display: 'flex',
    items: 'center',
    gap: 2,
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 'var(--jellypilot-line-height-24)',
    textAlign: 'left',
  },
]);

export const languageIcon = style({
  color: 'var(--jellypilot-color-secondary)',
  height: '1.125rem',
  width: '1.125rem',
});

export const clearButton = style({
  border: `1px solid var(--jellypilot-color-outline-variant)`,
  borderRadius: 'var(--jellypilot-radius-xl)',
  fontSize: 'var(--jellypilot-font-size-13)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  minWidth: 0,
  padding: `var(--jellypilot-space-1) var(--jellypilot-space-3)`,
  selectors: {
    '&:hover': {
      background: 'var(--jellypilot-color-surface-container-highest)',
      borderColor: 'var(--jellypilot-color-secondary)',
      color: 'var(--jellypilot-color-secondary)',
    },
  },
});

export const hidden = style({
  display: 'none',
});

export const languageGrid = style({
  display: 'grid',
  gap: 'var(--jellypilot-space-4)',
  gridTemplateColumns: '1fr',
  marginTop: 'var(--jellypilot-space-5)',
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
    },
  },
});

export const customField = style([
  atomic({
    display: 'flex',
    minWidth: 0,
    flexDirection: 'column',
  }),
]);

export const addRow = style([
  atomic({
    display: 'flex',
    gap: 2,
  }),
]);

export const addButton = style([
  atomic({
    display: 'inline-flex',
    height: 14,
    items: 'center',
    justify: 'center',
    rounded: '2xl',
    px: 4,
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-secondary-container)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    background: 'var(--jellypilot-color-secondary-container)',
    borderColor: 'var(--jellypilot-color-secondary)',
    cursor: 'pointer',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    minWidth: '5.5rem',
    transitionDuration: 'var(--jellypilot-duration-200)',
    transitionProperty: 'background-color, border-color, transform',
    selectors: {
      '&:hover': {
        background: 'var(--jellypilot-color-secondary-container)',
        borderColor: 'var(--jellypilot-color-secondary)',
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
      '&:disabled': {
        opacity: 0.5,
        pointerEvents: 'none',
      },
    },
  },
]);

export const plusIcon = style({
  height: 'var(--jellypilot-space-4)',
  marginRight: 'var(--jellypilot-space-1)',
  width: 'var(--jellypilot-space-4)',
});

export const empty = style({
  marginTop: 'var(--jellypilot-space-5)',
  borderRadius: 'var(--jellypilot-radius-2xl)',
  paddingInline: 'var(--jellypilot-space-4)',
  paddingBlock: 'var(--jellypilot-space-4)',
  textAlign: 'center',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  background: 'var(--jellypilot-color-surface-container-low)',
  border: `1px dashed var(--jellypilot-color-outline-variant)`,
  color: 'var(--jellypilot-color-on-surface-variant)',
  opacity: 0.8,
});

export const list = style({
  position: 'relative',
  marginTop: 'var(--jellypilot-space-5)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--jellypilot-space-2)',
  zIndex: 10,
});

export const item = style([
  atomic({
    display: 'flex',
    items: 'center',
    justify: 'space-between',
    gap: 3,
    rounded: 'xl',
    px: 4,
    py: 2.5,
  }),
  {
    background: 'var(--jellypilot-color-surface-container-highest)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    transitionProperty: 'background-color',
    selectors: {
      '&:hover': {
        background: 'var(--jellypilot-color-surface-container-high)',
      },
    },
  },
]);

export const itemPreview = style([
  atomic({
    display: 'flex',
    minWidth: 0,
    flexGrow: 1,
    items: 'center',
    gap: 3,
  }),
]);

export const indexBadge = style([
  atomic({
    display: 'flex',
    height: 6,
    width: 6,
    shrink: 0,
    items: 'center',
    justify: 'center',
    rounded: 'lg',
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 1,
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    borderRadius: 'var(--jellypilot-radius-lg)',
    fontFamily: 'var(--jellypilot-font-mono)',
    fontVariantNumeric: 'tabular-nums',
  },
]);

export const code = style({
  color: 'var(--jellypilot-color-on-surface)',
  flexShrink: 0,
  fontFamily: 'var(--jellypilot-font-mono)',
  fontSize: 'var(--jellypilot-font-size-14)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
});

export const itemLabel = style({
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  color: 'var(--jellypilot-color-on-surface-variant)',
  opacity: 0.8,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});
export const itemActions = style({
  display: 'flex',
  flexShrink: 0,
  alignItems: 'center',
  gap: 'var(--jellypilot-space-1_5)',
});

export const smallIconButton = style({
  border: `1px solid var(--jellypilot-color-outline-variant)`,
  borderRadius: 'var(--jellypilot-radius-lg)',
  height: 'var(--jellypilot-space-8)',
  width: 'var(--jellypilot-space-8)',
  selectors: {
    '&:hover': {
      borderColor: 'var(--jellypilot-color-secondary)',
      color: 'var(--jellypilot-color-secondary)',
    },
  },
});

export const deleteButton = style({
  selectors: {
    '&:hover': {
      borderColor: 'var(--jellypilot-color-error)',
      color: 'var(--jellypilot-color-error)',
    },
  },
});
