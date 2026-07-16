import { css, cva, cx } from '@styled-system/css';

const focusRing = {
  outline: '[2px solid {colors.primary}]',
  outlineOffset: '[2px]',
} as const;

export const button = cva({
  base: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontWeight: 'bold',
    border: 0,
    cursor: 'pointer',
    textDecoration: 'none',
    transitionDuration: '200',
    transitionProperty: '[background-color, border-color, color, box-shadow, filter, transform]',
    userSelect: 'none',
    verticalAlign: 'middle',
    _disabled: {
      opacity: '[0.5]',
      pointerEvents: 'none',
    },
    _focusVisible: focusRing,
  },
  variants: {
    size: {
      sm: {
        borderRadius: 'xl',
        fontSize: '12',
        gap: '1_5',
        lineHeight: '16',
        minHeight: '10',
        padding: '[0.5em 0.75em]',
      },
      md: {
        borderRadius: '2xl',
        fontSize: '14',
        gap: '2',
        lineHeight: '20',
        minHeight: '11',
        padding: '[0.875em 1.125em]',
      },
      lg: {
        borderRadius: '[1.25rem]',
        fontSize: '16',
        gap: '2_5',
        lineHeight: '24',
        minHeight: '[3.25rem]',
        padding: '[1.2em 1.45em]',
      },
    },
    variant: {
      primary: {
        backgroundImage: '[linear-gradient(90deg, {colors.primary}, {colors.primaryGradientEnd})]',
        boxShadow: '[0 10px 20px -10px {colors.primary/45}]',
        color: 'onPrimary',
        _hover: {
          boxShadow: '[0 14px 26px -10px {colors.primary/70}]',
          filter: '[brightness(1.1)]',
          transform: '[translateY(-2px)]',
        },
        _active: {
          transform: '[translateY(0) scale(0.96)]',
        },
      },
      secondary: {
        backgroundImage:
          '[linear-gradient(90deg, {colors.secondaryContainer}, {colors.secondaryGradientEnd})]',
        borderWidth: '1px',
        borderStyle: 'solid',
        borderColor: 'outlineVariant',
        boxShadow: 'md',
        color: 'onSecondaryContainer',
        _hover: {
          borderColor: 'outline',
          transform: '[translateY(-2px)]',
        },
        _active: {
          transform: '[translateY(0) scale(0.96)]',
        },
      },
      tonal: {
        backgroundImage:
          '[linear-gradient(90deg, {colors.secondaryContainer}, {colors.secondaryGradientEnd})]',
        borderWidth: '1px',
        borderStyle: 'solid',
        borderColor: 'outlineVariant',
        boxShadow: 'md',
        color: 'onSecondaryContainer',
        _hover: {
          borderColor: 'outline',
          transform: '[translateY(-2px)]',
        },
        _active: {
          transform: '[translateY(0) scale(0.96)]',
        },
      },
      outlined: {
        bg: '[transparent]',
        borderWidth: '1px',
        borderStyle: 'solid',
        borderColor: 'outline',
        color: 'onSurface',
        _hover: {
          bg: 'primary/5',
          borderColor: 'primary',
        },
        _active: {
          transform: '[scale(0.96)]',
        },
      },
      text: {
        bg: '[transparent]',
        color: 'secondary',
        _hover: {
          bg: 'secondary/10',
        },
        _active: {
          transform: '[scale(0.96)]',
        },
      },
    },
  },
  defaultVariants: {
    size: 'md',
    variant: 'primary',
  },
});

export const iconButton = cva({
  base: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: 'onSurfaceVariant',
    bg: '[transparent]',
    border: 0,
    cursor: 'pointer',
    padding: '0',
    transitionDuration: '200',
    transitionProperty: '[background-color, color, transform]',
    userSelect: 'none',
    _disabled: {
      opacity: '[0.5]',
      pointerEvents: 'none',
    },
    _focusVisible: focusRing,
    _hover: {
      bg: 'primary/10',
      color: 'onSurface',
    },
    _active: {
      transform: '[scale(0.96)]',
    },
  },
  variants: {
    size: {
      sm: {
        borderRadius: 'xl',
        height: '10',
        minHeight: '10',
        minWidth: '10',
        width: '10',
      },
      md: {
        borderRadius: '2xl',
        height: '11',
        minHeight: '11',
        minWidth: '11',
        width: '11',
      },
      lg: {
        borderRadius: '[1.25rem]',
        height: '[3.25rem]',
        minHeight: '[3.25rem]',
        minWidth: '[3.25rem]',
        width: '[3.25rem]',
      },
    },
  },
  defaultVariants: {
    size: 'md',
  },
});

export const buttonIcon = css({
  alignItems: 'center',
  display: 'inline-flex',
  flexShrink: '[0]',
  height: '[1lh]',
  justifyContent: 'center',
  width: '[1lh]',
});

export { cx };
