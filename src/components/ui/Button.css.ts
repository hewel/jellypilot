import { style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

const focusRing = {
  outline: `2px solid ${vars.color.primary}`,
  outlineOffset: '2px',
};

export const button = recipe({
  base: [
    sprinkles({
      display: 'inlineFlex',
      alignItems: 'center',
      justifyContent: 'center',
      fontWeight: 'bold',
    }),
    style({
      border: 0,
      cursor: 'pointer',
      textDecoration: 'none',
      transitionDuration: vars.duration['200'],
      transitionProperty: 'background-color, border-color, color, box-shadow, filter, transform',
      userSelect: 'none',
      verticalAlign: 'middle',
      selectors: {
        '&:disabled': {
          opacity: 0.5,
          pointerEvents: 'none',
        },
        '&:focus-visible': focusRing,
      },
    }),
  ],
  variants: {
    size: {
      sm: {
        borderRadius: vars.borderRadius.xl,
        fontSize: vars.fontSize['12'],
        gap: vars.space['1_5'],
        lineHeight: vars.lineHeight['16'],
        minHeight: vars.space['10'],
        padding: '0.5em 0.75em',
      },
      md: {
        borderRadius: vars.borderRadius['2xl'],
        fontSize: vars.fontSize['14'],
        gap: vars.space['2'],
        lineHeight: vars.lineHeight['20'],
        minHeight: vars.space['11'],
        padding: '0.875em 1.125em',
      },
      lg: {
        borderRadius: '1.25rem',
        fontSize: vars.fontSize['16'],
        gap: vars.space['2_5'],
        lineHeight: vars.lineHeight['24'],
        minHeight: '3.25rem',
        padding: '1.2em 1.45em',
      },
    },
    variant: {
      primary: {
        backgroundImage: `linear-gradient(90deg, ${vars.color.primary}, ${vars.color.primaryGradientEnd})`,
        boxShadow: `0 10px 20px -10px ${mix(vars.color.primary, 0.45)}`,
        color: vars.color.onPrimary,
        selectors: {
          '&:hover': {
            boxShadow: `0 14px 26px -10px ${mix(vars.color.primary, 0.7)}`,
            filter: 'brightness(1.1)',
            transform: 'translateY(-2px)',
          },
          '&:active': {
            transform: 'translateY(0) scale(0.96)',
          },
        },
      },
      secondary: {
        backgroundImage: `linear-gradient(90deg, ${vars.color.secondaryContainer}, ${vars.color.secondaryGradientEnd})`,
        border: `1px solid ${vars.color.outlineVariant}`,
        boxShadow: vars.shadow.md,
        color: vars.color.onSecondaryContainer,
        selectors: {
          '&:hover': {
            borderColor: vars.color.outline,
            transform: 'translateY(-2px)',
          },
          '&:active': {
            transform: 'translateY(0) scale(0.96)',
          },
        },
      },
      tonal: {
        backgroundImage: `linear-gradient(90deg, ${vars.color.secondaryContainer}, ${vars.color.secondaryGradientEnd})`,
        border: `1px solid ${vars.color.outlineVariant}`,
        boxShadow: vars.shadow.md,
        color: vars.color.onSecondaryContainer,
        selectors: {
          '&:hover': {
            borderColor: vars.color.outline,
            transform: 'translateY(-2px)',
          },
          '&:active': {
            transform: 'translateY(0) scale(0.96)',
          },
        },
      },
      outlined: {
        background: 'transparent',
        border: `1px solid ${vars.color.outline}`,
        color: vars.color.onSurface,
        selectors: {
          '&:hover': {
            background: mix(vars.color.primary, 0.05),
            borderColor: vars.color.primary,
          },
          '&:active': {
            transform: 'scale(0.96)',
          },
        },
      },
      text: {
        background: 'transparent',
        color: vars.color.secondary,
        selectors: {
          '&:hover': {
            background: mix(vars.color.secondary, 0.1),
          },
          '&:active': {
            transform: 'scale(0.96)',
          },
        },
      },
    },
  },
  defaultVariants: {
    size: 'md',
    variant: 'primary',
  },
});

export const iconButton = recipe({
  base: [
    sprinkles({
      display: 'inlineFlex',
      alignItems: 'center',
      justifyContent: 'center',
      color: 'onSurfaceVariant',
    }),
    style({
      background: 'transparent',
      border: 0,
      cursor: 'pointer',
      padding: 0,
      transitionDuration: vars.duration['200'],
      transitionProperty: 'background-color, color, transform',
      userSelect: 'none',
      selectors: {
        '&:disabled': {
          opacity: 0.5,
          pointerEvents: 'none',
        },
        '&:focus-visible': focusRing,
        '&:hover': {
          background: mix(vars.color.primary, 0.1),
          color: vars.color.onSurface,
        },
        '&:active': {
          transform: 'scale(0.96)',
        },
      },
    }),
  ],
  variants: {
    size: {
      sm: {
        borderRadius: vars.borderRadius.xl,
        height: vars.space['10'],
        minHeight: vars.space['10'],
        minWidth: vars.space['10'],
        width: vars.space['10'],
      },
      md: {
        borderRadius: vars.borderRadius['2xl'],
        height: vars.space['11'],
        minHeight: vars.space['11'],
        minWidth: vars.space['11'],
        width: vars.space['11'],
      },
      lg: {
        borderRadius: '1.25rem',
        height: '3.25rem',
        minHeight: '3.25rem',
        minWidth: '3.25rem',
        width: '3.25rem',
      },
    },
  },
  defaultVariants: {
    size: 'md',
  },
});

export const buttonIcon = style({
  alignItems: 'center',
  display: 'inline-flex',
  flexShrink: 0,
  height: '1lh',
  justifyContent: 'center',
  width: '1lh',
});
