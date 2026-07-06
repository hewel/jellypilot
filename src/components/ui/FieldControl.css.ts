import { style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const fieldControl = recipe({
  base: [
    sprinkles({
      color: 'onSurface',
      height: '14',
      px: '4',
      borderRadius: '2xl',
    }),
    style({
      borderStyle: 'solid',
      borderWidth: '1px',
      outline: 'none',
      transitionDuration: vars.duration['300'],
      transitionProperty: 'background-color, border-color, box-shadow',
      selectors: {
        '&::placeholder': {
          color: mix(vars.color.onSurfaceVariant, 0.5),
        },
        '&:disabled': {
          cursor: 'not-allowed',
          opacity: 0.5,
        },
      },
    }),
  ],
  variants: {
    variant: {
      filled: {
        backdropFilter: 'blur(4px)',
        background: mix(vars.color.surfaceContainerHighest, 0.3),
        borderColor: mix(vars.color.outlineVariant, 0.8),
        selectors: {
          '&:hover': {
            background: mix(vars.color.surfaceContainerHighest, 0.4),
            borderColor: mix(vars.color.secondary, 0.4),
          },
          '&:focus': {
            background: mix(vars.color.surfaceContainerHighest, 0.6),
            borderColor: vars.color.secondary,
            boxShadow: `0 0 0 4px ${mix(vars.color.secondary, 0.15)}`,
          },
        },
      },
      outlined: {
        background: 'transparent',
        borderColor: vars.color.outline,
        selectors: {
          '&:focus': {
            borderColor: vars.color.primary,
            boxShadow: `0 0 0 4px ${mix(vars.color.primary, 0.15)}`,
          },
        },
      },
    },
  },
  defaultVariants: {
    variant: 'filled',
  },
});
