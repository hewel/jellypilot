import { style, styleVariants } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const root = style([
  sprinkles({
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
  background: `linear-gradient(to bottom, color-mix(in srgb, ${vars.color.primaryContainer} 30%, transparent), ${vars.color.surface})`,
  height: '100%',
  width: '100%',
});

export const heroImage = style({
  filter: 'blur(20px) brightness(0.3)',
  height: '100%',
  objectFit: 'cover',
  transform: 'scale(1.1)',
  width: '100%',
});

export const scrim = style({
  background: `linear-gradient(to top, ${vars.color.surface}, color-mix(in srgb, ${vars.color.surface} 60%, transparent), transparent)`,
  inset: 0,
  position: 'absolute',
});

export const backButton = style({
  background: 'rgb(0 0 0 / 0.35)',
  backdropFilter: 'blur(12px)',
  borderColor: 'rgb(255 255 255 / 0.15)',
  borderRadius: vars.borderRadius.full,
  boxShadow: vars.shadow['2xl'],
  color: '#fff',
  left: vars.space['4'],
  position: 'absolute',
  top: vars.space['4'],
  transitionProperty: 'background-color, border-color, transform',
  zIndex: 20,
  selectors: {
    '&:hover': {
      background: 'rgb(0 0 0 / 0.5)',
      borderColor: 'rgb(255 255 255 / 0.3)',
    },
    '&:active': {
      transform: 'scale(0.96)',
    },
  },
  '@media': {
    'screen and (min-width: 1024px)': {
      left: vars.space['8'],
      top: vars.space['6'],
    },
    'screen and (min-width: 1280px)': {
      left: vars.space['10'],
    },
  },
});

export const backIcon = sprinkles({
  width: '4',
  height: '4',
});

export const content = style({
  alignItems: 'flex-end',
  display: 'flex',
  gap: vars.space['6'],
  height: '100%',
  padding: `0 ${vars.space['6']} ${vars.space['6']}`,
  position: 'relative',
  zIndex: 10,
  '@media': {
    'screen and (min-width: 1024px)': {
      gap: vars.space['8'],
      padding: `0 ${vars.space['10']} ${vars.space['8']}`,
    },
    'screen and (min-width: 1280px)': {
      gap: vars.space['10'],
      padding: `0 ${vars.space['12']} ${vars.space['10']}`,
    },
  },
});

export const artwork = style([
  sprinkles({
    position: 'relative',
    display: { base: 'none', sm: 'block' },
    flexShrink: '0',
    overflow: 'hidden',
    bg: 'surfaceContainerLowest',
    borderRadius: 'xl',
    boxShadow: '2xl',
  }),
  {
    background: `color-mix(in srgb, ${vars.color.surfaceContainerLowest} 70%, transparent)`,
    outline: '1px solid rgb(255 255 255 / 0.1)',
    outlineOffset: '-1px',
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
  sprinkles({
    display: 'flex',
    height: 'full',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '3',
    px: '4',
    textAlign: 'center',
    color: 'onSurfaceVariant',
  }),
]);

export const artworkFallbackIcon = sprinkles({
  display: 'flex',
  width: '12',
  height: '12',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: '2xl',
  color: 'secondary',
  bg: 'secondaryContainer',
});

export const artworkFallbackTitle = style([
  sprinkles({
    color: 'onSurface',
    fontSize: '13',
    lineHeight: '20',
    fontWeight: 'semibold',
  }),
  {
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
  background: `color-mix(in srgb, ${vars.color.surface} 70%, transparent)`,
  bottom: 0,
  height: vars.space['1'],
  left: 0,
  position: 'absolute',
  right: 0,
});

export const progressBar = style({
  background: vars.color.secondary,
  height: '100%',
});

export const copy = style({
  display: 'grid',
  flex: 1,
  gap: vars.space['3'],
  minWidth: 0,
  '@media': {
    'screen and (min-width: 1024px)': {
      gap: vars.space['4'],
    },
  },
});

export const titleBlock = style({
  display: 'grid',
  gap: vars.space['1'],
});

export const title = style({
  color: vars.color.onSurface,
  fontFamily: vars.font.display,
  fontSize: vars.fontSize['28'],
  fontWeight: vars.fontWeight.bold,
  letterSpacing: 0,
  lineHeight: vars.lineHeight['40'],
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
  color: vars.color.onSurfaceVariant,
  fontSize: vars.fontSize['14'],
  lineHeight: vars.lineHeight['20'],
  '@media': {
    'screen and (min-width: 1024px)': {
      fontSize: vars.fontSize['16'],
      lineHeight: vars.lineHeight['24'],
    },
  },
});

export const badges = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '2',
});

export const actions = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '3',
});
