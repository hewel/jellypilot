import { style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { sprinkles } from '../styles/sprinkles.css';
import { vars } from '../styles/vars.css';

const mix = (color: string, amount: number) =>
  `color-mix(in srgb, ${color} ${amount}%, transparent)`;

export const toast = recipe({
  base: [
    sprinkles({
      alignItems: 'center',
      borderRadius: 'xl',
      boxShadow: 'md',
      display: 'flex',
      mb: '4',
      p: '4',
      width: 'full',
    }),
    style({
      backdropFilter: 'blur(12px)',
      border: '1px solid',
      maxWidth: '24rem',
      pointerEvents: 'auto',
      transitionDuration: vars.duration['200'],
      transitionProperty: 'filter, opacity, transform',
      transitionTimingFunction: vars.easing.standard,
    }),
  ],
  variants: {
    level: {
      error: {
        background: mix(vars.color.errorContainer, 85),
        borderColor: mix(vars.color.error, 25),
        boxShadow: `${vars.shadow['2xl']}, 0 25px 50px -12px ${mix(vars.color.error, 10)}`,
        color: vars.color.onErrorContainer,
      },
      info: {
        background: mix(vars.color.surfaceContainerHigh, 90),
        borderColor: mix(vars.color.outlineVariant, 60),
        boxShadow: vars.shadow['2xl'],
        color: vars.color.onSurface,
      },
      success: {
        background: mix(vars.color.surfaceContainerHigh, 90),
        borderColor: mix(vars.color.tertiary, 20),
        boxShadow: `${vars.shadow['2xl']}, 0 25px 50px -12px ${mix(vars.color.tertiary, 5)}`,
        color: vars.color.onSurface,
      },
      warning: {
        background: mix(vars.color.warningContainer, 85),
        borderColor: mix(vars.color.warning, 25),
        boxShadow: `${vars.shadow['2xl']}, 0 25px 50px -12px ${mix(vars.color.warning, 10)}`,
        color: vars.color.onWarningContainer,
      },
    },
  },
});

export const visible = style({
  animation: `fadeIn ${vars.duration['200']} ${vars.easing.emphasized} forwards`,
  filter: 'blur(0)',
  opacity: 1,
  transform: 'translateY(0)',
});

export const exiting = style({
  filter: 'blur(2px)',
  opacity: 0,
  transform: `translateY(${vars.space['1']})`,
});

export const iconWrap = sprinkles({
  alignItems: 'center',
  display: 'inlineFlex',
  flexShrink: '0',
  justifyContent: 'center',
});

export const icon = recipe({
  base: style({
    height: vars.space['5'],
    width: vars.space['5'],
  }),
  variants: {
    level: {
      error: { color: vars.color.error },
      info: { color: vars.color.primary },
      success: { color: vars.color.tertiary },
      warning: { color: vars.color.warning },
    },
  },
});

export const message = style([
  sprinkles({
    flexGrow: '1',
    fontSize: '14',
    fontWeight: 'normal',
    lineHeight: '20',
    ml: '3',
  }),
  {
    color: vars.color.onSurfaceVariant,
    overflowWrap: 'break-word',
  },
]);

export const closeButton = style([
  sprinkles({
    alignItems: 'center',
    borderRadius: 'full',
    display: 'inlineFlex',
    height: '10',
    justifyContent: 'center',
    ml: 'auto',
    p: '1_5',
    width: '10',
  }),
  {
    background: 'transparent',
    border: '0',
    color: 'currentColor',
    cursor: 'pointer',
    marginBottom: `calc(${vars.space['1_5']} * -1)`,
    marginRight: `calc(${vars.space['1_5']} * -1)`,
    marginTop: `calc(${vars.space['1_5']} * -1)`,
    transitionDuration: vars.duration['150'],
    transitionProperty: 'background-color',
    transitionTimingFunction: vars.easing.standard,
    selectors: {
      '&:hover': {
        background: mix(vars.color.onSurface, 10),
      },
    },
  },
]);

export const closeIcon = style({
  height: vars.space['4'],
  width: vars.space['4'],
});

export const srOnly = style({
  border: '0',
  clip: 'rect(0, 0, 0, 0)',
  height: '1px',
  margin: '-1px',
  overflow: 'hidden',
  padding: 0,
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: '1px',
});
