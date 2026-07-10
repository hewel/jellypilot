import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const root = style([
  atomic({
    display: 'grid',
    gap: 4,
  }),
]);

export const header = style([
  atomic({
    display: 'flex',
    items: 'center',
    justify: 'between',
    gap: 3,
    px: 1,
  }),
]);
export const count = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontFamily: 'var(--jellypilot-font-mono)',
  fontSize: 'var(--jellypilot-font-size-11)',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'var(--jellypilot-font-weight-semibold)',
});

export const checkboxRoot = style([
  atomic({
    display: 'inline-flex',
    items: 'center',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    cursor: 'pointer',
    fontSize: 'var(--jellypilot-font-size-11)',
    gap: 'var(--jellypilot-space-2_5)',
    fontWeight: '700',
    lineHeight: 1,
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    transitionProperty: 'opacity',
    userSelect: 'none',
    verticalAlign: 'top',
    selectors: {
      '&:disabled': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const checkbox = style([
  atomic({
    display: 'inline-flex',
    items: 'center',
    justify: 'center',
    shrink: 0,
    rounded: 'lg',
  }),
  {
    alignItems: 'center',
    background: 'var(--jellypilot-color-surface-container-high)',
    border: '1px solid var(--jellypilot-color-outline)',
    color: 'var(--jellypilot-color-on-primary)',
    fontSize: 'var(--jellypilot-font-size-11)',
    height: '1.375rem',
    justifyContent: 'center',
    transitionDuration: 'var(--jellypilot-duration-200)',
    transitionProperty: 'background-color, border-color',
    width: '1.375rem',
    selectors: {
      '&:hover': {
        borderColor: 'var(--jellypilot-color-primary)',
      },
      '&[data-state="checked"], &[data-state="indeterminate"]': {
        background: 'var(--jellypilot-color-primary)',
        borderColor: 'var(--jellypilot-color-primary)',
      },
      '&[data-focus-visible]': {
        outline: '2px solid var(--jellypilot-color-secondary)',
        outlineOffset: '1px',
      },
    },
  },
]);

export const indicator = style([
  atomic({
    display: 'flex',
    items: 'center',
    justify: 'center',
  }),
  {
    fontWeight: '900',
  },
]);

export const checkboxLabel = style({
  cursor: 'pointer',
  userSelect: 'none',
});

export const log = style([
  atomic({
    overflowY: 'auto',
    rounded: '2xl',
  }),
  {
    backdropFilter: 'none',
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    display: 'grid',
    gap: 'var(--jellypilot-space-2_5)',
    maxHeight: 'inherit',
    padding: 'var(--jellypilot-space-3)',
  },
]);

export const compactLog = style({
  maxHeight: '14rem',
});

export const expandedLog = style({
  maxHeight: '24rem',
});

export const empty = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontFamily: 'var(--jellypilot-font-mono)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  paddingBlock: 'var(--jellypilot-space-10)',
  textAlign: 'center',
});

export const entry = style([
  atomic({
    overflow: 'hidden',
    position: 'relative',
    rounded: 'xl',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontFamily: 'var(--jellypilot-font-mono)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    paddingBottom: 'var(--jellypilot-space-2)',
    paddingLeft: 'var(--jellypilot-space-3_5)',
    paddingRight: 'var(--jellypilot-space-3_5)',
    paddingTop: 'var(--jellypilot-space-2)',
    transitionProperty: 'background-color, border-color',
    selectors: {
      '&:hover': {
        background: 'var(--jellypilot-color-surface-container-highest)',
        borderColor: 'var(--jellypilot-color-outline)',
      },
    },
  },
]);

export const entryInner = style([
  atomic({
    display: 'flex',
    items: 'center',
    wrap: 'wrap',
  }),
  {
    gap: 'var(--jellypilot-space-1_5) var(--jellypilot-space-3)',
    position: 'relative',
    zIndex: 10,
  },
]);

export const time = style({
  color: 'var(--jellypilot-color-outline)',
  fontWeight: '700',
  userSelect: 'none',
});

export const badge = style({
  border: '1px solid currentColor',
  borderRadius: 'var(--jellypilot-radius-md)',
  fontSize: 'var(--jellypilot-font-size-10)',
  fontWeight: '700',
  letterSpacing: '0.05em',
  padding: 'var(--jellypilot-space-0_5) var(--jellypilot-space-2)',
  userSelect: 'none',
});

export const badgeTrace = style({
  background: 'var(--jellypilot-color-surface-container-highest)',
  borderColor: 'var(--jellypilot-color-outline-variant)',
  color: 'var(--jellypilot-color-outline)',
});

export const badgeDebug = style({
  background: 'var(--jellypilot-color-surface-container-highest)',
  borderColor: 'var(--jellypilot-color-outline)',
  color: 'var(--jellypilot-color-on-surface-variant)',
});

export const badgeInfo = style({
  background: 'var(--jellypilot-color-secondary-container)',
  borderColor: 'var(--jellypilot-color-secondary)',
  color: 'var(--jellypilot-color-secondary)',
});

export const badgeWarn = style({
  background: 'var(--jellypilot-color-warning-container)',
  borderColor: 'var(--jellypilot-color-warning)',
  color: 'var(--jellypilot-color-warning)',
});

export const badgeError = style({
  background: 'var(--jellypilot-color-error-container)',
  borderColor: 'var(--jellypilot-color-error)',
  color: 'var(--jellypilot-color-error)',
});

export const message = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontWeight: '500',
  overflowWrap: 'anywhere',
});

export const actions = style([
  atomic({
    display: 'flex',
    items: 'center',
    justify: 'flex-end',
    gap: 3,
    px: 1,
    wrap: 'wrap',
  }),
]);

export const status = style([
  atomic({
    fontSize: 'var(--jellypilot-font-size-11)',
    fontWeight: '700',
    lineHeight: 'var(--jellypilot-line-height-16)',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const statusCopied = style({
  color: 'var(--jellypilot-color-tertiary)',
});

export const statusFailed = style({
  color: 'var(--jellypilot-color-error)',
});

export const actionButton = style({
  border: `1px solid var(--jellypilot-color-outline-variant)`,
  borderRadius: 'var(--jellypilot-radius-xl)',
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-11)',
  fontWeight: '700',
  letterSpacing: '0.08em',
  lineHeight: 'var(--jellypilot-line-height-16)',
  textTransform: 'uppercase',
  selectors: {
    '&:hover': {
      background: 'var(--jellypilot-color-surface-container-high)',
      borderColor: 'var(--jellypilot-color-secondary)',
      color: 'var(--jellypilot-color-secondary)',
    },
  },
});

export const dangerActionButton = style({
  selectors: {
    '&:hover': {
      background: 'var(--jellypilot-color-error)',
      borderColor: 'var(--jellypilot-color-error)',
      color: 'var(--jellypilot-color-background)',
    },
  },
});
