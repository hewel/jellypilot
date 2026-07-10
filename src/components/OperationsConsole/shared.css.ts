import { atomic } from '@jellypilot/atomic-css';
import { style, styleVariants } from '@vanilla-extract/css';

export const sectionIcon = styleVariants({
  primary: {
    color: 'var(--jellypilot-color-primary)',
    height: 'var(--jellypilot-space-5)',
    width: 'var(--jellypilot-space-5)',
  },
  secondary: {
    color: 'var(--jellypilot-color-secondary)',
    height: 'var(--jellypilot-space-5)',
    width: 'var(--jellypilot-space-5)',
  },
  plain: {
    height: 'var(--jellypilot-space-6)',
    width: 'var(--jellypilot-space-6)',
  },
});

export const grid3 = style([
  atomic({
    display: 'grid',
    gap: 4,
  }),
  {
    gridTemplateColumns: '1fr',
    '@media': {
      'screen and (min-width: 768px)': {
        gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
      },
    },
  },
]);

export const tile = style([
  atomic({
    position: 'relative',
    overflow: 'hidden',
    rounded: '2xl',
    p: 4,
  }),
  {
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    boxSizing: 'border-box',
  },
]);

export const span2 = style({
  '@media': {
    'screen and (min-width: 768px)': {
      gridColumn: 'span 2 / span 2',
    },
  },
});

export const tileWatermark = style({
  opacity: 0.05,
  padding: 'var(--jellypilot-space-3)',
  position: 'absolute',
  right: 0,
  top: 0,
});

export const watermarkIcon = style({
  width: 'var(--jellypilot-space-12)',
  height: 'var(--jellypilot-space-12)',
});

export const overline = style([
  atomic({
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const value = style([
  atomic({
    mt: 1.5,
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 'var(--jellypilot-line-height-24)',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const monoValue = style([
  atomic({
    mt: 1.5,
  }),
  {
    color: 'var(--jellypilot-color-secondary)',
    fontFamily: 'var(--jellypilot-font-mono)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const bodyText = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  opacity: 0.85,
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
});

export const warning = style([
  atomic({
    display: 'flex',
    items: 'flex-start',
    gap: 2,
    mt: 2,
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-warning)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
  },
]);

export const warningIcon = style({
  flexShrink: 0,
  height: 'var(--jellypilot-space-3)',
  marginTop: 'var(--jellypilot-space-0_5)',
  width: 'var(--jellypilot-space-3)',
});

export const actionRow = style([
  atomic({
    mt: 6,
    display: 'flex',
    wrap: 'wrap',
    items: 'center',
    gap: 3,
  }),
]);

export const refreshButton = style({
  background: 'var(--jellypilot-color-surface-container-high)',
  border: `1px solid var(--jellypilot-color-outline-variant)`,
  borderRadius: 'var(--jellypilot-radius-xl)',
  marginLeft: 'auto',
  selectors: {
    '&:hover': {
      borderColor: 'var(--jellypilot-color-secondary)',
      color: 'var(--jellypilot-color-secondary)',
    },
  },
});

export const mutedOutlinedButton = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  selectors: {
    '&:hover': {
      borderColor: 'var(--jellypilot-color-primary)',
      color: 'var(--jellypilot-color-on-surface)',
    },
  },
});

export const stack4 = style([
  atomic({
    display: 'grid',
    gap: 4,
  }),
]);

export const fieldset = style({
  border: 0,
  display: 'grid',
  gap: 'var(--jellypilot-space-3)',
  gridTemplateColumns: '1fr',
  margin: 0,
  padding: 0,
});

export const choice = style([
  atomic({
    rounded: '2xl',
    px: 4,
    py: 3,
    textAlign: 'left',
  }),
  {
    cursor: 'pointer',
    background: 'var(--jellypilot-color-surface-container-high)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    transitionDuration: 'var(--jellypilot-duration-300)',
    transitionProperty: 'background-color, border-color, transform',
    selectors: {
      '&:active': {
        transform: 'scale(0.96)',
      },
      '&:hover': {
        background: 'var(--jellypilot-color-surface-container-highest)',
        borderColor: 'var(--jellypilot-color-primary)',
      },
    },
  },
]);

export const choiceIdle = style({
  background: 'var(--jellypilot-color-surface-container-high)',
  color: 'var(--jellypilot-color-on-surface)',
});

export const choiceSelected = style({
  background: 'var(--jellypilot-color-primary-container)',
  borderColor: 'var(--jellypilot-color-primary)',
  color: 'var(--jellypilot-color-on-primary-container)',
  fontWeight: 'var(--jellypilot-font-weight-semibold)',
});

export const choiceTitle = style([
  atomic({
    display: 'block',
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 'var(--jellypilot-line-height-24)',
  },
]);

export const choiceDescription = style([
  atomic({
    display: 'block',
    mt: 1,
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    opacity: 0.8,
  },
]);

export const saving = style([
  atomic({
    display: 'flex',
    items: 'center',
    gap: 1.5,
  }),
  {
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    fontWeight: 'var(--jellypilot-font-weight-semibold)',
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  },
]);

export const pingDot = style({
  animation: 'ping 1s cubic-bezier(0, 0, 0.2, 1) infinite',
  background: 'var(--jellypilot-color-secondary)',
  borderRadius: 'var(--jellypilot-radius-full)',
  height: 'var(--jellypilot-space-1_5)',
  width: 'var(--jellypilot-space-1_5)',
});

export const errorPanel = style([
  atomic({
    rounded: '2xl',
    px: 4,
    py: 3,
    fontWeight: 'semibold',
  }),
  {
    background: 'var(--jellypilot-color-error-container)',
    border: `1px solid var(--jellypilot-color-error)`,
    color: 'var(--jellypilot-color-on-error-container)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
  },
]);
