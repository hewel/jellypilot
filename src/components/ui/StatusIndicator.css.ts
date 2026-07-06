import { keyframes } from '@vanilla-extract/css';
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

export const root = sprinkles({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
});

export const statusDot = recipe({
  base: {
    animation: `${pulseKeyframes} 2s cubic-bezier(0.4, 0, 0.6, 1) infinite`,
    borderRadius: vars.borderRadius.full,
    height: vars.space['2_5'],
    width: vars.space['2_5'],
  },
  variants: {
    connected: {
      true: {
        background: vars.color.tertiary,
        boxShadow: `0 0 8px ${mix(vars.color.tertiary, 0.5)}`,
      },
      false: {
        background: vars.color.error,
        boxShadow: `0 0 8px ${mix(vars.color.error, 0.5)}`,
      },
    },
  },
});

export const text = recipe({
  base: sprinkles({ fontWeight: 'bold' }),
  variants: {
    connected: {
      true: sprinkles({ color: 'onSurface' }),
      false: sprinkles({ color: 'error' }),
    },
  },
});
