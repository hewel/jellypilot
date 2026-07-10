import { atomic } from '@jellypilot/atomic-css';
import { projectTheme } from '@jellypilot/ui/theme/project';
import { style, styleVariants } from '@vanilla-extract/css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const root = style({
  display: 'grid',
  gap: projectTheme.space['6'],
  overflow: 'hidden',
  position: 'relative',
});

export const bareRoot = style({
  display: 'grid',
  gap: projectTheme.space['5'],
});

export const hoverGlow = style({
  background: `linear-gradient(90deg, ${mix(projectTheme.color.primary, 0.05)}, ${mix(projectTheme.color.secondary, 0.05)})`,
  inset: 0,
  opacity: 0,
  pointerEvents: 'none',
  position: 'absolute',
  transitionDuration: projectTheme.duration['500'],
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
  gap: projectTheme.space['1'],
});

export const headerBare = style({
  paddingRight: '9rem',
});

export const headerFramed = style({
  maxWidth: '70%',
});

export const eyebrow = style([
  atomic({
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: '700',
  }),
  {
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const titleRow = atomic({
  display: 'flex',
  alignItems: 'center',
  gap: 3,
});

export const title = style({
  color: projectTheme.color.onSurface,
  fontFamily: projectTheme.font.display,
  fontWeight: projectTheme.fontWeight.bold,
  letterSpacing: 0,
  overflow: 'hidden',
  paddingRight: projectTheme.space['2'],
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const titleBare = atomic({
  fontSize: 'var(--jellypilot-font-size-20)',
  lineHeight: 'var(--jellypilot-line-height-28)',
});

export const titleFramed = atomic({
  fontSize: 'var(--jellypilot-font-size-24)',
  lineHeight: 'var(--jellypilot-line-height-32)',
});

export const equalizer = style([
  atomic({
    display: 'flex',
    height: 6,
    width: 8,
    flexShrink: '0',
    alignItems: 'flex-end',
    gap: 1.5,
    pb: 1,
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
  borderRadius: projectTheme.borderRadius.full,
  height: '100%',
  transformOrigin: 'bottom',
  width: projectTheme.space['1_5'],
  willChange: 'transform',
});

export const wavePrimary = style({
  background: projectTheme.color.primary,
});

export const waveSecondary = style({
  background: projectTheme.color.secondary,
});

export const waveTiming = styleVariants({
  a: { animationDuration: '0.8s' },
  b: { animationDelay: '0.15s', animationDuration: '0.6s' },
  c: { animationDelay: '0.3s', animationDuration: '0.9s' },
  d: { animationDelay: '0.45s', animationDuration: '0.7s' },
});

export const subtitle = atomic({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 'var(--jellypilot-line-height-20)',
  fontWeight: '500',
});

export const badgePlacement = style({
  position: 'absolute',
  right: 0,
  top: 0,
});

export const panel = style([
  atomic({
    position: 'relative',
    borderRadius: '3xl',
    p: 4,
    zIndex: 10,
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(projectTheme.color.surfaceContainerLowest, 0.5),
    border: `1px solid ${projectTheme.color.outlineVariant}`,
  },
]);

export const timeRow = style([
  atomic({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    mb: 2.5,
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-11)',
    fontWeight: '600',
  }),
  {
    fontFamily: projectTheme.font.mono,
    fontVariantNumeric: 'tabular-nums',
  },
]);

export const emptyTrack = style({
  background: mix(projectTheme.color.surfaceContainerHigh, 0.6),
  borderRadius: projectTheme.borderRadius.full,
  height: projectTheme.space['2'],
});

export const sliderRoot = style([
  atomic({
    display: 'flex',
    width: 'full',
    flexDirection: 'column',
    gap: 2.5,
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
  atomic({
    position: 'relative',
    display: 'flex',
    height: 10,
    alignItems: 'center',
  }),
  {
    cursor: 'pointer',
  },
]);

export const sliderTrack = style([
  atomic({
    flexGrow: '1',
    overflow: 'hidden',
    borderRadius: 'full',
  }),
  {
    background: mix(projectTheme.color.surfaceContainerHighest, 0.8),
    border: `1px solid ${mix(projectTheme.color.outlineVariant, 0.3)}`,
    height: projectTheme.space['2_5'],
  },
]);

export const sliderRange = style({
  borderRadius: projectTheme.borderRadius.full,
  height: '100%',
  transitionDuration: projectTheme.duration['150'],
  transitionProperty: 'width, transform',
});

export const primaryRange = style({
  background: `linear-gradient(90deg, ${projectTheme.color.primary}, ${projectTheme.color.primaryGradientEnd})`,
  boxShadow: '0 0 10px rgba(79, 70, 229, 0.35)',
});

export const secondaryRange = style({
  background: `linear-gradient(90deg, ${projectTheme.color.secondary}, ${projectTheme.color.primary})`,
  boxShadow: '0 0 8px rgba(129, 140, 248, 0.4)',
});

export const thumb = style([
  atomic({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 'full',
  }),
  {
    background: projectTheme.color.onSurface,
    border: `2px solid ${projectTheme.color.surfaceContainerLowest}`,
    boxShadow: '0 10px 15px -3px rgb(0 0 0 / 0.5)',
    cursor: 'grab',
    height: '1.375rem',
    outline: 'none',
    transitionDuration: projectTheme.duration['200'],
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
        boxShadow: `0 0 0 2px ${mix(projectTheme.color.primary, 0.5)}`,
      },
    },
  },
]);

export const controls = style([
  atomic({
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    gap: 3,
    zIndex: 10,
  }),
]);

export const controlsBare = atomic({
  justifyContent: 'center',
});

export const controlsFramed = atomic({
  flexWrap: 'wrap',
  gap: 4,
});

export const iconButton = style({
  background: mix(projectTheme.color.surfaceContainerHigh, 0.3),
  border: `1px solid ${mix(projectTheme.color.outlineVariant, 0.6)}`,
  borderRadius: projectTheme.borderRadius.full,
  selectors: {
    '&:hover': {
      background: mix(projectTheme.color.secondary, 0.05),
      borderColor: projectTheme.color.secondary,
      color: projectTheme.color.secondary,
    },
  },
});

export const stopButton = style({
  selectors: {
    '&:hover': {
      background: mix(projectTheme.color.error, 0.05),
      borderColor: projectTheme.color.error,
      color: projectTheme.color.error,
    },
  },
});

export const playPauseButton = style({
  borderRadius: projectTheme.borderRadius.full,
  minWidth: '8rem',
  overflow: 'hidden',
  position: 'relative',
});

export const iconSlot = style({
  display: 'inline-grid',
  height: projectTheme.space['5'],
  placeItems: 'center',
  position: 'relative',
  width: projectTheme.space['5'],
});

export const contextualIcon = style({
  height: projectTheme.space['5'],
  position: 'absolute',
  transitionDuration: projectTheme.duration['200'],
  transitionProperty: 'filter, opacity, transform',
  transitionTimingFunction: projectTheme.easing.standard,
  width: projectTheme.space['5'],
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
  color: projectTheme.color.secondary,
});

export const errorIcon = style({
  color: projectTheme.color.error,
});

export const actionLabel = style({
  fontWeight: projectTheme.fontWeight.bold,
  letterSpacing: 0,
});

export const squareIcon = style({
  fill: 'currentColor',
  height: projectTheme.space['4'],
  width: projectTheme.space['4'],
});

export const pillButton = style({
  borderRadius: projectTheme.borderRadius.full,
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
    gap: projectTheme.space['3'],
    '@media': {
      'screen and (min-width: 640px)': {
        gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
      },
    },
  },
]);

export const volumePanel = style([
  panel,
  atomic({
    display: 'flex',
    alignItems: 'center',
    gap: 3,
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
  borderRadius: projectTheme.borderRadius.xl,
  flexShrink: 0,
  selectors: {
    '&:hover': {
      background: mix(projectTheme.color.secondary, 0.15),
      borderColor: mix(projectTheme.color.secondary, 0.2),
      color: projectTheme.color.secondary,
    },
  },
});

export const volumeValue = style({
  color: projectTheme.color.secondary,
  filter: 'drop-shadow(0 0 6px rgba(129, 140, 248, 0.15))',
  fontFamily: projectTheme.font.mono,
  fontSize: projectTheme.fontSize['13'],
  fontVariantNumeric: 'tabular-nums',
  fontWeight: projectTheme.fontWeight.semibold,
  textAlign: 'right',
  width: projectTheme.space['12'],
});

export const card = style({
  display: 'grid',
  gap: projectTheme.space['6'],
  overflow: 'hidden',
  position: 'relative',
});
