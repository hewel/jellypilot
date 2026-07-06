import { style, styleVariants } from '@vanilla-extract/css';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const root = style({
  display: 'grid',
  gap: vars.space['6'],
  overflow: 'hidden',
  position: 'relative',
});

export const bareRoot = style({
  display: 'grid',
  gap: vars.space['5'],
});

export const hoverGlow = style({
  background: `linear-gradient(90deg, ${mix(vars.color.primary, 0.05)}, ${mix(vars.color.secondary, 0.05)})`,
  inset: 0,
  opacity: 0,
  pointerEvents: 'none',
  position: 'absolute',
  transitionDuration: vars.duration['500'],
  transitionProperty: 'opacity',
  selectors: {
    [`${root}:hover &`]: {
      opacity: 1,
    },
  },
});

export const header = style({
  position: 'relative',
  zIndex: 10,
});

export const headerCopy = style({
  display: 'grid',
  gap: vars.space['1'],
});

export const headerBare = style({
  paddingRight: '9rem',
});

export const headerFramed = style({
  maxWidth: '70%',
});

export const eyebrow = style([
  sprinkles({
    color: 'secondary',
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const titleRow = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '3',
});

export const title = style({
  color: vars.color.onSurface,
  fontFamily: vars.font.display,
  fontWeight: vars.fontWeight.bold,
  letterSpacing: 0,
  overflow: 'hidden',
  paddingRight: vars.space['2'],
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const titleBare = sprinkles({
  fontSize: '20',
  lineHeight: '28',
});

export const titleFramed = sprinkles({
  fontSize: '24',
  lineHeight: '32',
});

export const equalizer = style([
  sprinkles({
    display: 'flex',
    height: '6',
    width: '8',
    flexShrink: '0',
    alignItems: 'flex-end',
    gap: '1_5',
    pb: '1',
  }),
  {
    userSelect: 'none',
  },
]);

export const waveBar = style({
  animationDirection: 'alternate',
  animationIterationCount: 'infinite',
  animationName: 'wave-bounce',
  animationTimingFunction: 'ease-in-out',
  borderRadius: vars.borderRadius.full,
  height: '100%',
  transformOrigin: 'bottom',
  width: vars.space['1_5'],
  willChange: 'transform',
});

export const wavePrimary = style({
  background: vars.color.primary,
});

export const waveSecondary = style({
  background: vars.color.secondary,
});

export const waveTiming = styleVariants({
  a: { animationDuration: '0.8s' },
  b: { animationDelay: '0.15s', animationDuration: '0.6s' },
  c: { animationDelay: '0.3s', animationDuration: '0.9s' },
  d: { animationDelay: '0.45s', animationDuration: '0.7s' },
});

export const subtitle = sprinkles({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'medium',
});

export const badgePlacement = style({
  position: 'absolute',
  right: 0,
  top: 0,
});

export const panel = style([
  sprinkles({
    position: 'relative',
    borderRadius: '3xl',
    p: '4',
    boxShadow: 'inner',
    zIndex: '10',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerLowest, 0.5),
    border: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const timeRow = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    mb: '2_5',
    color: 'onSurfaceVariant',
    fontSize: '11',
    fontWeight: 'semibold',
  }),
  {
    fontFamily: vars.font.mono,
    fontVariantNumeric: 'tabular-nums',
  },
]);

export const emptyTrack = style({
  background: mix(vars.color.surfaceContainerHigh, 0.6),
  borderRadius: vars.borderRadius.full,
  height: vars.space['2'],
});

export const sliderRoot = style([
  sprinkles({
    display: 'flex',
    width: 'full',
    flexDirection: 'column',
    gap: '2_5',
  }),
  {
    selectors: {
      '&:disabled': {
        opacity: 0.5,
      },
    },
  },
]);

export const sliderControl = style([
  sprinkles({
    position: 'relative',
    display: 'flex',
    height: '10',
    alignItems: 'center',
  }),
  {
    cursor: 'pointer',
  },
]);

export const sliderTrack = style([
  sprinkles({
    flexGrow: '1',
    overflow: 'hidden',
    borderRadius: 'full',
  }),
  {
    background: mix(vars.color.surfaceContainerHighest, 0.8),
    border: `1px solid ${mix(vars.color.outlineVariant, 0.3)}`,
    height: vars.space['2_5'],
  },
]);

export const sliderRange = style({
  borderRadius: vars.borderRadius.full,
  height: '100%',
  transitionDuration: vars.duration['150'],
  transitionProperty: 'width, transform',
});

export const primaryRange = style({
  background: `linear-gradient(90deg, ${vars.color.primary}, ${vars.color.primaryGradientEnd})`,
  boxShadow: '0 0 10px rgba(79, 70, 229, 0.35)',
});

export const secondaryRange = style({
  background: `linear-gradient(90deg, ${vars.color.secondary}, ${vars.color.primary})`,
  boxShadow: '0 0 8px rgba(129, 140, 248, 0.4)',
});

export const thumb = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 'full',
    boxShadow: 'lg',
  }),
  {
    background: vars.color.onSurface,
    border: `2px solid ${vars.color.surfaceContainerLowest}`,
    boxShadow: '0 10px 15px -3px rgb(0 0 0 / 0.5)',
    cursor: 'grab',
    height: '1.375rem',
    outline: 'none',
    transitionDuration: vars.duration['200'],
    transitionProperty: 'box-shadow, transform',
    width: '1.375rem',
    selectors: {
      '&:hover': {
        boxShadow: '0 0 12px rgba(255, 255, 255, 0.4)',
        transform: 'scale(1.1)',
      },
      '&:active': {
        cursor: 'grabbing',
      },
      '&[data-focus-visible]': {
        boxShadow: `0 0 0 2px ${mix(vars.color.primary, 0.5)}`,
      },
    },
  },
]);

export const controls = style([
  sprinkles({
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    gap: '3',
    zIndex: '10',
  }),
]);

export const controlsBare = sprinkles({
  justifyContent: 'center',
});

export const controlsFramed = sprinkles({
  flexWrap: 'wrap',
  gap: '4',
});

export const iconButton = style({
  background: mix(vars.color.surfaceContainerHigh, 0.3),
  border: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
  borderRadius: vars.borderRadius.full,
  selectors: {
    '&:hover': {
      background: mix(vars.color.secondary, 0.05),
      borderColor: vars.color.secondary,
      color: vars.color.secondary,
    },
  },
});

export const stopButton = style({
  selectors: {
    '&:hover': {
      background: mix(vars.color.error, 0.05),
      borderColor: vars.color.error,
      color: vars.color.error,
    },
  },
});

export const playPauseButton = style({
  borderRadius: vars.borderRadius.full,
  minWidth: '8rem',
  overflow: 'hidden',
  position: 'relative',
});

export const iconSlot = style({
  display: 'inline-grid',
  height: vars.space['5'],
  placeItems: 'center',
  position: 'relative',
  width: vars.space['5'],
});

export const contextualIcon = style({
  height: vars.space['5'],
  position: 'absolute',
  transitionDuration: vars.duration['200'],
  transitionProperty: 'filter, opacity, transform',
  transitionTimingFunction: vars.easing.standard,
  width: vars.space['5'],
});

export const iconVisible = style({
  filter: 'blur(0)',
  opacity: 1,
  transform: 'scale(1)',
});

export const iconHidden = style({
  filter: 'blur(4px)',
  opacity: 0,
  transform: 'scale(0.25)',
});

export const iconDropShadow = style({
  filter: 'drop-shadow(0 2px 4px rgba(0, 0, 0, 0.3))',
});

export const secondaryIcon = style({
  color: vars.color.secondary,
});

export const errorIcon = style({
  color: vars.color.error,
});

export const actionLabel = style({
  fontWeight: vars.fontWeight.bold,
  letterSpacing: 0,
});

export const squareIcon = style({
  fill: 'currentColor',
  height: vars.space['4'],
  width: vars.space['4'],
});

export const pillButton = style({
  borderRadius: vars.borderRadius.full,
});

export const playIcon = style({
  fill: 'currentColor',
  height: '1.125rem',
  width: '1.125rem',
});

export const selectPanel = style([
  panel,
  {
    display: 'grid',
    gap: vars.space['3'],
    '@media': {
      'screen and (min-width: 640px)': {
        gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
      },
    },
  },
]);

export const volumePanel = style([
  panel,
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    gap: '3',
  }),
  {
    '@media': {
      'screen and (max-width: 639px)': {
        flexDirection: 'column',
      },
    },
  },
]);

export const muteButton = style({
  border: '1px solid transparent',
  borderRadius: vars.borderRadius.xl,
  flexShrink: 0,
  selectors: {
    '&:hover': {
      background: mix(vars.color.secondary, 0.15),
      borderColor: mix(vars.color.secondary, 0.2),
      color: vars.color.secondary,
    },
  },
});

export const volumeValue = style({
  color: vars.color.secondary,
  filter: 'drop-shadow(0 0 6px rgba(129, 140, 248, 0.15))',
  fontFamily: vars.font.mono,
  fontSize: vars.fontSize['13'],
  fontVariantNumeric: 'tabular-nums',
  fontWeight: vars.fontWeight.semibold,
  textAlign: 'right',
  width: vars.space['12'],
});

export const card = style({
  display: 'grid',
  gap: vars.space['6'],
  overflow: 'hidden',
  position: 'relative',
});
