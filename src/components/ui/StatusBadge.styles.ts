import { cva } from '@styled-system/css';

export const statusBadge = cva({
  base: {
    display: 'inline-flex',
    alignItems: 'center',
    flexShrink: '[0]',
    gap: '1_5',
    borderRadius: 'full',
    px: '3',
    py: '1',
    fontSize: '11',
    fontWeight: 'bold',
    lineHeight: '16',
    borderStyle: 'solid',
    borderWidth: '1px',
    letterSpacing: '[0.08em]',
    textTransform: 'uppercase',
    userSelect: 'none',
  },
  variants: {
    variant: {
      success: {
        bg: 'tertiaryContainer/20',
        borderColor: 'tertiary/30',
        boxShadow: '[0 0 8px {colors.tertiary/12}]',
        color: 'tertiary',
      },
      warning: {
        bg: 'warningContainer/20',
        borderColor: 'warning/30',
        boxShadow: '[0 0 8px {colors.warning/12}]',
        color: 'warning',
      },
      error: {
        bg: 'errorContainer/20',
        borderColor: 'error/30',
        boxShadow: '[0 0 8px {colors.error/12}]',
        color: 'error',
      },
      neutral: {
        bg: 'surfaceContainerHighest/30',
        borderColor: 'outlineVariant/60',
        color: 'onSurfaceVariant',
        fontWeight: 'semibold',
      },
    },
  },
  defaultVariants: {
    variant: 'neutral',
  },
});

export const statusDot = cva({
  base: {
    borderRadius: 'full',
    height: '1_5',
    width: '1_5',
  },
  variants: {
    variant: {
      success: {
        animation: '[pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
        bg: 'tertiary',
      },
      warning: {
        animation: '[pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
        bg: 'warning',
      },
      error: {
        animation: '[pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
        bg: 'error',
      },
      neutral: {
        bg: 'onSurfaceVariant/60',
      },
    },
  },
  defaultVariants: {
    variant: 'neutral',
  },
});
