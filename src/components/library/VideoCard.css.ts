import { style, styleVariants } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const pulse = {
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
};

export const card = style([
  sprinkles({
    display: 'block',
    overflow: 'hidden',
    borderRadius: '2xl',
    boxShadow: 'xl',
  }),
  {
    backdropFilter: 'blur(12px)',
    background:
      'linear-gradient(135deg, color-mix(in srgb, var(--jellypilot-color-surface-container-low) 50%, transparent) 0%, color-mix(in srgb, var(--jellypilot-color-surface) 70%, transparent) 100%)',
    border: `1px solid color-mix(in srgb, ${vars.color.outlineVariant} 80%, transparent)`,
    color: 'inherit',
    padding: 0,
    textDecoration: 'none',
    transitionDuration: vars.duration['300'],
    transitionProperty: 'background-color, border-color, box-shadow, transform',
    selectors: {
      '&:hover': {
        borderColor: `color-mix(in srgb, ${vars.color.primary} 50%, transparent)`,
      },
      '&:focus-visible': {
        outline: 'none',
        boxShadow: `0 0 0 2px color-mix(in srgb, ${vars.color.secondary} 70%, transparent), ${vars.shadow.xl}`,
      },
      '&:active': {
        transform: 'scale(0.96)',
      },
    },
  },
]);

export const artwork = style([
  sprinkles({
    position: 'relative',
    overflow: 'hidden',
    bg: 'surfaceContainerLowest',
  }),
  {
    borderBottom: `1px solid ${vars.color.outlineVariant}`,
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
  sprinkles({
    display: 'flex',
    height: 'full',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '2',
    px: '4',
    textAlign: 'center',
    color: 'onSurfaceVariant',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const fallbackIcon = sprinkles({
  width: '5',
  height: '5',
});

export const image = style({
  height: '100%',
  objectFit: 'cover',
  outline: '1px solid rgb(255 255 255 / 0.1)',
  outlineOffset: '-1px',
  width: '100%',
});

export const favoriteBadge = style([
  sprinkles({
    position: 'absolute',
    display: 'inlineFlex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '7',
    height: '7',
    color: 'onSecondary',
    bg: 'secondary',
    borderRadius: 'full',
    boxShadow: 'lg',
  }),
  {
    right: vars.space['2'],
    top: vars.space['2'],
  },
]);

export const favoriteIcon = sprinkles({
  width: '4',
  height: '4',
});

export const body = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  px: '4',
  pt: '2',
  pb: '3',
});

export const copy = style([
  sprinkles({
    minWidth: '0',
    flexGrow: '1',
  }),
  {
    display: 'grid',
    gap: vars.space['1'],
  },
]);

export const title = style([
  sprinkles({
    color: 'onSurface',
    fontSize: '16',
    lineHeight: '24',
    fontWeight: 'semibold',
  }),
  {
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 1,
  },
]);

export const subtitle = style([
  sprinkles({
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: `color-mix(in srgb, ${vars.color.onSurfaceVariant} 80%, transparent)`,
  },
]);

export const playedBadge = sprinkles({
  display: 'inlineFlex',
  alignItems: 'center',
  justifyContent: 'center',
  width: '5',
  height: '5',
  flexShrink: '0',
  color: 'tertiary',
});

export const playedIcon = sprinkles({
  width: '4',
  height: '4',
});

export const skeleton = style(pulse);

export const skeletonBody = style([
  sprinkles({
    px: '4',
    pt: '2',
    pb: '3',
  }),
  {
    display: 'grid',
    gap: vars.space['2'],
  },
]);

export const skeletonTitle = style([
  sprinkles({
    height: '4',
    borderRadius: 'md',
  }),
  {
    ...pulse,
    background: `color-mix(in srgb, ${vars.color.surfaceContainerHigh} 80%, transparent)`,
    width: '80%',
  },
]);

export const skeletonSubtitle = style([
  sprinkles({
    height: '3',
    borderRadius: 'md',
  }),
  {
    ...pulse,
    background: `color-mix(in srgb, ${vars.color.surfaceContainerHigh} 60%, transparent)`,
    width: '60%',
  },
]);
