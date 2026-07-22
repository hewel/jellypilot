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
    letterSpacing: '8',
    textTransform: 'uppercase',
    userSelect: 'none',
  },
  variants: {
    variant: {
      success: {
        bg: 'tertiaryContainer/20',
        borderColor: 'tertiary/30',
        color: 'tertiary',
      },
      warning: {
        bg: 'warningContainer/20',
        borderColor: 'warning/30',
        color: 'warning',
      },
      error: {
        bg: 'errorContainer/20',
        borderColor: 'error/30',
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
        animation: '[pulse 2s {easings.inOut} infinite]',
        bg: 'tertiary',
      },
      warning: {
        animation: '[pulse 2s {easings.inOut} infinite]',
        bg: 'warning',
      },
      error: {
        animation: '[pulse 2s {easings.inOut} infinite]',
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
