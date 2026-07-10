import { atomic } from '@jellypilot/atomic-css';
import { style, styleVariants } from '@vanilla-extract/css';

const pulse = {
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
};

export const card = style([
  atomic({
    display: 'block',
    overflow: 'hidden',
    rounded: '2xl',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-low)',
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    color: 'inherit',
    padding: 0,
    textDecoration: 'none',
    transitionDuration: 'var(--jellypilot-duration-300)',
    transitionProperty: 'background-color, border-color, transform',
    selectors: {
      '&:hover': {
        borderColor: 'var(--jellypilot-color-primary)',
      },
      '&:focus-visible': {
        outline: `2px solid var(--jellypilot-color-secondary)`,
        outlineOffset: '2px',
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
    },
  },
]);

export const artwork = style([
  atomic({
    position: 'relative',
    overflow: 'hidden',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-lowest)',
    borderBottom: `1px solid var(--jellypilot-color-outline-variant)`,
  },
]);

export const aspect = styleVariants({
  poster: {
    aspectRatio: '2 / 3',
  },
  video: {
    aspectRatio: '16 / 9',
  },
});

export const fallback = style([
  atomic({
    display: 'flex',
    height: 'full',
    flexDirection: 'column',
    items: 'center',
    justify: 'center',
    gap: 2,
    px: 4,
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    textAlign: 'center',
  },
]);

export const fallbackIcon = style([
  atomic({
    width: 5,
    height: 5,
  }),
]);

export const image = style({
  height: '100%',
  objectFit: 'cover',
  outline: '1px solid var(--jellypilot-color-outline)',
  outlineOffset: '-1px',
  width: '100%',
});

export const favoriteBadge = style([
  atomic({
    position: 'absolute',
    display: 'inline-flex',
    items: 'center',
    justify: 'center',
    width: 7,
    height: 7,
    rounded: 'full',
  }),
  {
    color: 'var(--jellypilot-color-on-secondary)',
    background: 'var(--jellypilot-color-secondary)',
    right: 'var(--jellypilot-space-2)',
    top: 'var(--jellypilot-space-2)',
  },
]);

export const favoriteIcon = style([
  atomic({
    width: 4,
    height: 4,
  }),
]);

export const body = style([
  atomic({
    display: 'flex',
    items: 'center',
    gap: 2,
    px: 4,
    pt: 2,
    pb: 3,
  }),
]);

export const copy = style([
  atomic({
    minWidth: 0,
    grow: 1,
  }),
  {
    display: 'grid',
    gap: 'var(--jellypilot-space-1)',
  },
]);

export const title = style([
  atomic({
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 1,
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 'var(--jellypilot-line-height-24)',
  },
]);

export const subtitle = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
});

export const playedBadge = style([
  atomic({
    display: 'inline-flex',
    items: 'center',
    justify: 'center',
    width: 5,
    height: 5,
    shrink: 0,
  }),
  {
    color: 'var(--jellypilot-color-tertiary)',
  },
]);

export const playedIcon = style([
  atomic({
    width: 4,
    height: 4,
  }),
]);

export const skeleton = style(pulse);

export const skeletonBody = style([
  atomic({
    px: 4,
    pt: 2,
    pb: 3,
  }),
  {
    display: 'grid',
    gap: 'var(--jellypilot-space-2)',
  },
]);

export const skeletonTitle = style([
  atomic({
    height: 4,
    rounded: 'md',
  }),
  {
    ...pulse,
    background: 'var(--jellypilot-color-surface-container-high)',
    width: '80%',
    opacity: 0.85,
  },
]);

export const skeletonSubtitle = style([
  atomic({
    height: 3,
    rounded: 'md',
  }),
  {
    ...pulse,
    background: 'var(--jellypilot-color-surface-container-high)',
    width: '60%',
    opacity: 0.7,
  },
]);
