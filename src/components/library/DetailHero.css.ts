import { atomic } from '@jellypilot/atomic-css';
import { globalStyle, style, styleVariants } from '@vanilla-extract/css';

export const root = style([
  atomic({
    position: 'relative',
    overflow: 'hidden',
  }),
  {
    height: 'clamp(280px, 44vh, 560px)',
  },
]);

export const mediaLayer = style({
  inset: 0,
  position: 'absolute',
});

export const fallbackBackdrop = style({
  background: 'var(--jellypilot-color-surface)',
  height: '100%',
  width: '100%',
});

export const heroImage = style({
  height: '100%',
  objectFit: 'cover',
  width: '100%',
});

export const scrim = style({
  background: 'var(--jellypilot-color-surface)',
  inset: 0,
  opacity: 0.5,
  position: 'absolute',
});

export const backButton = style([
  atomic({
    position: 'absolute',
    rounded: 'full',
    zIndex: 20,
    left: 'var(--jellypilot-space-4)',
    top: 'var(--jellypilot-space-4)',
  }),
  {
    border: '1px solid var(--jellypilot-color-outline)',
    color: 'var(--jellypilot-color-on-surface)',
    transitionProperty: 'background-color, border-color, transform',
    backgroundColor: 'var(--jellypilot-color-surface-container-low)',
    '@media': {
      'screen and (min-width: 1024px)': {
        left: 'var(--jellypilot-space-8)',
        top: 'var(--jellypilot-space-6)',
      },
      'screen and (min-width: 1280px)': {
        left: 'var(--jellypilot-space-10)',
      },
    },
    selectors: {
      '&:hover': {
        backgroundColor: 'var(--jellypilot-color-surface-container-high)',
        borderColor: 'var(--jellypilot-color-outline-variant)',
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
    },
  },
]);

export const backIcon = style([
  atomic({
    width: 4,
    height: 4,
  }),
]);

export const content = style([
  atomic({
    alignItems: 'flex-end',
    display: 'flex',
    gap: 6,
  }),
  {
    height: '100%',
    padding: '0 var(--jellypilot-space-6) var(--jellypilot-space-6)',
    position: 'relative',
    zIndex: 10,
    '@media': {
      'screen and (min-width: 1024px)': {
        gap: 'var(--jellypilot-space-8)',
        padding: '0 var(--jellypilot-space-10) var(--jellypilot-space-8)',
      },
      'screen and (min-width: 1280px)': {
        gap: 'var(--jellypilot-space-10)',
        padding: '0 var(--jellypilot-space-12) var(--jellypilot-space-10)',
      },
    },
  },
]);

export const artwork = style([
  atomic({
    position: 'relative',
    flexShrink: 0,
    overflow: 'hidden',
    rounded: 'xl',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-lowest)',
    display: 'none',
    outline: '1px solid var(--jellypilot-color-outline)',
    outlineOffset: '-1px',
    '@media': {
      'screen and (min-width: 640px)': {
        display: 'block',
      },
    },
  },
]);

export const artworkAspect = styleVariants({
  poster: {
    aspectRatio: '2 / 3',
  },
  landscape: {
    aspectRatio: '16 / 9',
  },
});

export const artworkWidth = styleVariants({
  poster: {
    width: '140px',
    '@media': {
      'screen and (min-width: 1024px)': { width: '190px' },
      'screen and (min-width: 1536px)': { width: '220px' },
    },
  },
  landscape: {
    width: '200px',
    '@media': {
      'screen and (min-width: 1024px)': { width: '280px' },
      'screen and (min-width: 1536px)': { width: '340px' },
    },
  },
});

export const artworkFallback = style([
  atomic({
    display: 'flex',
    height: 'full',
    flexDirection: 'column',
    items: 'center',
    justify: 'center',
    gap: 3,
    px: 4,
    textAlign: 'center',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
  },
]);

export const artworkFallbackIcon = style([
  atomic({
    display: 'flex',
    width: 12,
    height: 12,
    items: 'center',
    justify: 'center',
    rounded: '2xl',
  }),
  {
    color: 'var(--jellypilot-color-secondary)',
    backgroundColor: 'var(--jellypilot-color-secondary-container)',
  },
]);

export const artworkFallbackTitle = style([
  atomic({
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-13)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 3,
  },
]);

export const artworkImage = style({
  height: '100%',
  objectFit: 'cover',
  width: '100%',
});

export const progressTrack = style({
  background: 'var(--jellypilot-color-surface)',
  bottom: 0,
  height: 'var(--jellypilot-space-1)',
  left: 0,
  position: 'absolute',
  right: 0,
  borderRadius: 0,
});

globalStyle(`${progressTrack} [data-part="fill"]`, {
  background: 'var(--jellypilot-color-secondary)',
});

export const copy = style([
  atomic({
    display: 'grid',
    gap: 3,
  }),
  {
    flex: 1,
    minWidth: 0,
    '@media': {
      'screen and (min-width: 1024px)': {
        gap: 'var(--jellypilot-space-4)',
      },
    },
  },
]);

export const titleBlock = style([
  atomic({
    display: 'grid',
    gap: 1,
  }),
]);

export const title = style({
  color: 'var(--jellypilot-color-on-surface)',
  fontFamily: 'var(--jellypilot-font-display)',
  fontSize: 'var(--jellypilot-font-size-28)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  letterSpacing: 0,
  lineHeight: 'var(--jellypilot-line-height-40)',
  '@media': {
    'screen and (min-width: 1024px)': {
      fontSize: '2.625rem',
      lineHeight: '3.125rem',
    },
    'screen and (min-width: 1280px)': {
      fontSize: '3rem',
      lineHeight: '3.5rem',
    },
  },
});

export const subtitle = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 'var(--jellypilot-line-height-20)',
  '@media': {
    'screen and (min-width: 1024px)': {
      fontSize: 'var(--jellypilot-font-size-16)',
      lineHeight: 'var(--jellypilot-line-height-24)',
    },
  },
});

export const badges = style([
  atomic({
    display: 'flex',
    wrap: 'wrap',
    items: 'center',
    gap: 2,
  }),
]);

export const actions = style([
  atomic({
    display: 'flex',
    wrap: 'wrap',
    items: 'center',
    gap: 3,
  }),
]);
