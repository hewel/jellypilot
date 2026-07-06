import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const content = style({
  display: 'grid',
  gap: vars.space['2'],
});

export const title = style([
  sprinkles({
    color: 'onSurface',
    fontSize: '14',
    lineHeight: '20',
    fontWeight: 'semibold',
  }),
  {
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 2,
  },
]);

export const meta = style([
  sprinkles({
    color: 'onSurfaceVariant',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    fontVariantNumeric: 'tabular-nums',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const genres = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '1',
});

export const genre = style([
  sprinkles({
    borderRadius: 'full',
    px: '2',
    py: '0_5',
    color: 'onSurfaceVariant',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    background: `color-mix(in srgb, ${vars.color.surfaceContainerHighest} 70%, transparent)`,
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const overview = style([
  sprinkles({
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: `color-mix(in srgb, ${vars.color.onSurfaceVariant} 90%, transparent)`,
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 3,
  },
]);

export const progressTrack = style([
  sprinkles({
    overflow: 'hidden',
    width: 'full',
    height: '1',
    borderRadius: 'full',
  }),
  {
    background: `color-mix(in srgb, ${vars.color.surfaceContainerHighest} 70%, transparent)`,
  },
]);

export const progressBar = style({
  background: vars.color.secondary,
  height: '100%',
});

export const watchedText = style([
  sprinkles({
    mt: '1',
    color: 'onSurfaceVariant',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    fontVariantNumeric: 'tabular-nums',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const states = style([
  sprinkles({
    display: 'flex',
    flexWrap: 'wrap',
    gap: '3',
    color: 'onSurfaceVariant',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.05em',
    paddingTop: vars.space['0_5'],
    textTransform: 'uppercase',
  },
]);

export const state = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '1',
});

export const played = sprinkles({
  color: 'tertiary',
});

export const favorite = sprinkles({
  color: 'secondary',
});

export const icon = style({
  height: vars.space['3_5'],
  width: vars.space['3_5'],
});

export const popover = style([
  sprinkles({
    zIndex: '100',
    borderRadius: '2xl',
    p: '4',
    boxShadow: '2xl',
  }),
  {
    backdropFilter: 'blur(12px)',
    background: vars.color.surfaceContainerLowest,
    border: `1px solid ${vars.color.outlineVariant}`,
    maxWidth: 'min(90vw, 24rem)',
    width: '20rem',
  },
]);

export const loading = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '2',
    py: '3',
    color: 'onSurfaceVariant',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const spinner = style([
  {
    animation: 'spin 1s linear infinite',
    height: vars.space['4'],
    width: vars.space['4'],
  },
]);

export const error = style([
  sprinkles({
    py: '2',
    textAlign: 'center',
    fontSize: '12',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    color: `color-mix(in srgb, ${vars.color.error} 90%, transparent)`,
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);
