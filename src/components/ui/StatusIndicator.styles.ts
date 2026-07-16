import { css, cva } from '@styled-system/css';

export const root = css({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
});

export const statusDot = cva({
  base: {
    animation: '[pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
    borderRadius: 'full',
    height: '2_5',
    width: '2_5',
  },
  variants: {
    connected: {
      true: {
        bg: 'tertiary',
        boxShadow: '[0 0 8px {colors.tertiary/50}]',
      },
      false: {
        bg: 'error',
        boxShadow: '[0 0 8px {colors.error/50}]',
      },
    },
  },
});

export const text = cva({
  base: {
    fontWeight: 'bold',
  },
  variants: {
    connected: {
      true: { color: 'onSurface' },
      false: { color: 'error' },
    },
  },
});
