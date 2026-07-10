import { projectBreakpoints, projectTheme } from '@jellypilot/ui/theme/project';
import { style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

const mix = (color: string, amount: number) =>
  `color-mix(in srgb, ${color} ${amount}%, transparent)`;

export const trigger = style({
  position: 'relative',
});

export const triggerIcon = style({
  height: projectTheme.space['5'],
  width: projectTheme.space['5'],
});

export const statusDot = recipe({
  base: style({
    borderRadius: projectTheme.borderRadius.full,
    height: projectTheme.space['2'],
    position: 'absolute',
    right: projectTheme.space['1'],
    top: projectTheme.space['1'],
    width: projectTheme.space['2'],
  }),
  variants: {
    status: {
      idle: {
        background: projectTheme.color.outlineVariant,
      },
      offline: {
        background: projectTheme.color.error,
        boxShadow: `0 0 8px ${projectTheme.color.error}`,
      },
      paused: {
        background: projectTheme.color.tertiary,
        boxShadow: `0 0 8px ${projectTheme.color.tertiary}`,
      },
      playing: {
        background: projectTheme.color.tertiary,
        boxShadow: `0 0 8px ${projectTheme.color.tertiary}`,
      },
      unknown: {
        background: projectTheme.color.outlineVariant,
      },
    },
  },
});

export const backdrop = style({
  background: 'rgb(0 0 0 / 0.7)',
  backdropFilter: 'blur(4px)',
  inset: 0,
  position: 'fixed',
  transitionDuration: projectTheme.duration['300'],
  transitionProperty: 'backdrop-filter, background-color, opacity',
  zIndex: projectTheme.zIndex['50'],
  selectors: {
    '&[data-state="closed"]': {
      opacity: 0,
    },
    '&[data-state="open"]': {
      opacity: 1,
    },
  },
});

export const positioner = style({
  display: 'flex',
  inset: 0,
  justifyContent: 'flex-end',
  position: 'fixed',
  zIndex: projectTheme.zIndex['50'],
});

export const content = style({
  backdropFilter: 'blur(24px)',
  background: mix(projectTheme.color.surfaceContainerLow, 60),
  borderLeft: `1px solid ${mix(projectTheme.color.outlineVariant, 30)}`,
  borderTopLeftRadius: '2rem',
  borderBottomLeftRadius: '2rem',
  boxShadow: projectTheme.shadow['2xl'],
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  overflow: 'hidden',
  transitionDuration: projectTheme.duration['200'],
  transitionProperty: 'opacity, transform',
  transitionTimingFunction: projectTheme.easing.standard,
  width: '100%',
  selectors: {
    '&[data-state="closed"]': {
      opacity: 0,
      transform: `translateX(${projectTheme.space['3']})`,
    },
    '&[data-state="open"]': {
      opacity: 1,
      transform: 'translateX(0)',
    },
  },
  '@media': {
    [`screen and (min-width: ${projectBreakpoints.sm})`]: {
      width: '28rem',
    },
  },
});

export const header = style({
  alignItems: 'center',
  borderBottom: `1px solid ${mix(projectTheme.color.outlineVariant, 20)}`,
  display: 'flex',
  justifyContent: 'space-between',
  padding: `${projectTheme.space['4']} ${projectTheme.space['5']}`,
});

export const title = style({
  color: projectTheme.color.onSurface,
  fontSize: projectTheme.fontSize['18'],
  fontWeight: projectTheme.fontWeight.bold,
  lineHeight: projectTheme.lineHeight['24'],
});

export const description = style({
  color: mix(projectTheme.color.onSurfaceVariant, 70),
  fontSize: projectTheme.fontSize['12'],
  lineHeight: projectTheme.lineHeight['16'],
  marginTop: projectTheme.space['0_5'],
});

export const closeIcon = style({
  height: projectTheme.space['5'],
  width: projectTheme.space['5'],
});

export const body = style({
  flex: 1,
  overflowY: 'auto',
  padding: `${projectTheme.space['4']} ${projectTheme.space['5']}`,
});

export const srOnlyClose = style({
  position: 'absolute',
  width: '1px',
  height: '1px',
  overflow: 'hidden',
  clip: 'rect(0 0 0 0)',
});
