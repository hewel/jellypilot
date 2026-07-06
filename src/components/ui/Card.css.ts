import { style, styleVariants } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

/**
 * Non-atomic card surface treatment: multi-stop gradients and compound shadows
 * that utility atoms cannot express cleanly.
 */
export const cardSurface = styleVariants({
  elevated: {
    backgroundImage:
      'linear-gradient(135deg, rgba(28, 32, 48, 0.45) 0%, rgba(17, 19, 28, 0.65) 100%)',
    boxShadow: '0 25px 50px -12px rgba(0, 0, 0, 0.6), 0 0 40px -10px rgba(79, 70, 229, 0.12)',
    selectors: {
      '&:hover': {
        boxShadow: '0 30px 60px -12px rgba(0, 0, 0, 0.75), 0 0 50px -5px rgba(79, 70, 229, 0.22)',
      },
    },
  },
  filled: {
    backgroundImage:
      'linear-gradient(135deg, rgba(21, 24, 35, 0.5) 0%, rgba(11, 13, 20, 0.7) 100%)',
  },
  outlined: {},
});

export const card = recipe({
  base: [
    sprinkles({
      position: 'relative',
      overflow: 'hidden',
    }),
    style({
      borderStyle: 'solid',
      borderWidth: '1px',
      transitionDuration: vars.duration['300'],
      transitionProperty: 'background-color, border-color, box-shadow',
    }),
  ],
  variants: {
    variant: {
      elevated: {
        backdropFilter: 'blur(24px)',
        background:
          'color-mix(in srgb, var(--jellypilot-color-surface-container-low) 45%, transparent)',
        borderColor: 'color-mix(in srgb, var(--jellypilot-color-primary) 20%, transparent)',
        borderRadius: vars.borderRadius['4xl'],
        boxShadow: vars.shadow['2xl'],
        padding: vars.space['6'],
        selectors: {
          '&:hover': {
            background:
              'color-mix(in srgb, var(--jellypilot-color-surface-container-low) 60%, transparent)',
            borderColor: 'color-mix(in srgb, var(--jellypilot-color-primary) 35%, transparent)',
          },
        },
      },
      filled: {
        backdropFilter: 'blur(12px)',
        background: 'color-mix(in srgb, var(--jellypilot-color-surface) 50%, transparent)',
        borderColor: 'color-mix(in srgb, var(--jellypilot-color-outline-variant) 80%, transparent)',
        borderRadius: vars.borderRadius['2xl'],
        boxShadow: vars.shadow.xl,
        padding: vars.space['4'],
      },
      outlined: {
        background: 'transparent',
        borderColor: vars.color.outlineVariant,
        borderRadius: '1.75rem',
        padding: vars.space['6'],
        selectors: {
          '&:hover': {
            borderColor: 'color-mix(in srgb, var(--jellypilot-color-outline) 40%, transparent)',
          },
        },
      },
    },
    padding: {
      default: {},
      none: {
        padding: 0,
      },
    },
  },
  defaultVariants: {
    padding: 'default',
    variant: 'filled',
  },
});

export const tintOverlay = style({
  background: 'color-mix(in srgb, var(--jellypilot-color-surface-tint) 3%, transparent)',
  borderRadius: 'inherit',
  inset: 0,
  pointerEvents: 'none',
  position: 'absolute',
});

export const content = style({
  position: 'relative',
  zIndex: 10,
});
