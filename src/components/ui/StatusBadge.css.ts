import { keyframes, style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

const pulseKeyframes = keyframes({
  '0%, 100%': {
    opacity: 1,
  },
  '50%': {
    opacity: 0.5,
  },
});

export const statusBadge = recipe({
  base: [
    sprinkles({
      display: 'inlineFlex',
      alignItems: 'center',
      flexShrink: '0',
      gap: '1_5',
      borderRadius: 'full',
      px: '3',
      py: '1',
      fontSize: '11',
      fontWeight: 'bold',
      lineHeight: '16',
    }),
    style({
      borderStyle: 'solid',
      borderWidth: '1px',
      letterSpacing: '0.08em',
      textTransform: 'uppercase',
      userSelect: 'none',
    }),
  ],
  variants: {
    variant: {
      success: {
        background: mix(vars.color.tertiaryContainer, 0.2),
        borderColor: mix(vars.color.tertiary, 0.3),
        boxShadow: `0 0 8px ${mix(vars.color.tertiary, 0.12)}`,
        color: vars.color.tertiary,
      },
      warning: {
        background: mix(vars.color.warningContainer, 0.2),
        borderColor: mix(vars.color.warning, 0.3),
        boxShadow: `0 0 8px ${mix(vars.color.warning, 0.12)}`,
        color: vars.color.warning,
      },
      error: {
        background: mix(vars.color.errorContainer, 0.2),
        borderColor: mix(vars.color.error, 0.3),
        boxShadow: `0 0 8px ${mix(vars.color.error, 0.12)}`,
        color: vars.color.error,
      },
      neutral: {
        background: mix(vars.color.surfaceContainerHighest, 0.3),
        borderColor: mix(vars.color.outlineVariant, 0.6),
        color: vars.color.onSurfaceVariant,
        fontWeight: vars.fontWeight.semibold,
      },
    },
  },
  defaultVariants: {
    variant: 'neutral',
  },
});

export const statusDot = recipe({
  base: {
    borderRadius: vars.borderRadius.full,
    height: vars.space['1_5'],
    width: vars.space['1_5'],
  },
  variants: {
    variant: {
      success: {
        animation: `${pulseKeyframes} 2s cubic-bezier(0.4, 0, 0.6, 1) infinite`,
        background: vars.color.tertiary,
      },
      warning: {
        animation: `${pulseKeyframes} 2s cubic-bezier(0.4, 0, 0.6, 1) infinite`,
        background: vars.color.warning,
      },
      error: {
        animation: `${pulseKeyframes} 2s cubic-bezier(0.4, 0, 0.6, 1) infinite`,
        background: vars.color.error,
      },
      neutral: {
        background: mix(vars.color.onSurfaceVariant, 0.6),
      },
    },
  },
  defaultVariants: {
    variant: 'neutral',
  },
});
