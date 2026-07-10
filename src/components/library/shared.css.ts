import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const statusCard = atomic({
  display: 'grid',
  gap: 5,
});

export const statusContent = atomic({
  display: 'flex',
  items: 'start',
  gap: 4,
});

export const statusIcon = style([
  atomic({
    display: 'flex',
    width: 12,
    height: 12,
    shrink: 0,
    items: 'center',
    justify: 'center',
    rounded: '2xl',
  }),
  {
    background: `color-mix(in srgb, var(--jellypilot-color-tertiary-container) 25%, transparent)`,
    border: `1px solid color-mix(in srgb, var(--jellypilot-color-tertiary) 30%, transparent)`,
    color: 'var(--jellypilot-color-tertiary)',
  },
]);

export const iconMd = atomic({
  width: 6,
  height: 6,
});

export const statusCopy = atomic({
  display: 'grid',
  gap: 2,
});

export const statusTitle = style([
  atomic({
    fontWeight: 'bold',
  }),
  {
    fontFamily: 'var(--jellypilot-font-display)',
    fontSize: 'var(--jellypilot-font-size-24)',
    lineHeight: 'var(--jellypilot-line-height-32)',
  },
]);

export const statusDescription = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 'var(--jellypilot-line-height-20)',
});

export const row = atomic({
  display: 'grid',
  gap: 3,
});

export const rowTitle = style({
  color: 'var(--jellypilot-color-on-surface)',
  fontSize: 'var(--jellypilot-font-size-22)',
  lineHeight: 'var(--jellypilot-line-height-28)',
  fontWeight: 'bold',
});

export const videoGrid = style([
  atomic({
    display: 'grid',
    gap: 3,
  }),
  {
    '@media': {
      'screen and (min-width: 640px)': {
        gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
      },
      'screen and (min-width: 1280px)': {
        gridTemplateColumns: 'repeat(4, minmax(0, 1fr))',
      },
      'screen and (min-width: 1536px)': {
        gridTemplateColumns: 'repeat(6, minmax(0, 1fr))',
      },
    },
  },
]);

export const subtitleLink = style({
  color: 'var(--jellypilot-color-secondary)',
  textDecoration: 'none',
  textUnderlineOffset: 'var(--jellypilot-space-1)',
  selectors: {
    '&:hover': {
      textDecoration: 'underline',
    },
  },
});

export const userDataControls = atomic({
  display: 'flex',
  wrap: 'wrap',
  gap: 3,
});

export const userDataActions = userDataControls;

export const pillButton = style({
  borderRadius: 'var(--jellypilot-border-radius-full)',
});

export const favoriteSelected = style({
  borderColor: `color-mix(in srgb, var(--jellypilot-color-error) 30%, transparent)`,
});

export const playedSelected = style({
  borderColor: `color-mix(in srgb, var(--jellypilot-color-tertiary) 30%, transparent)`,
});

export const iconSm = atomic({
  width: 4,
  height: 4,
});

export const favoriteIcon = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
});

export const favoriteIconSelected = style({
  color: 'var(--jellypilot-color-error)',
  fill: 'var(--jellypilot-color-error)',
});

export const playedIcon = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
});

export const playedIconSelected = style({
  color: 'var(--jellypilot-color-tertiary)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
});

export const spinIcon = style([
  atomic({
    width: 4,
    height: 4,
  }),
  {
    animation: 'spin 1s linear infinite',
    color: 'var(--jellypilot-color-secondary)',
  },
]);

export const errorText = style({
  color: 'var(--jellypilot-color-error)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
});
