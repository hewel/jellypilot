import { cva } from '@styled-system/css';

export const fieldControl = cva({
  base: {
    color: 'onSurface',
    height: '14',
    px: '4',
    borderRadius: '2xl',
    borderStyle: 'solid',
    borderWidth: '1px',
    outline: 'none',
    transitionDuration: '300',
    transitionProperty: '[background-color, border-color, box-shadow]',
    _placeholder: {
      color: 'onSurfaceVariant/50',
    },
    _disabled: {
      cursor: 'not-allowed',
      opacity: '[0.5]',
    },
  },
  variants: {
    variant: {
      filled: {
        bg: 'surfaceContainerHighest/30',
        borderColor: 'outlineVariant/80',
        _hover: {
          bg: 'surfaceContainerHighest/40',
          borderColor: 'secondary/40',
        },
        _focus: {
          bg: 'surfaceContainerHighest/60',
          borderColor: 'secondary',
          boxShadow: '[0 0 0 4px {colors.secondary/15}]',
        },
      },
      outlined: {
        bg: '[transparent]',
        borderColor: 'outline',
        _focus: {
          borderColor: 'primary',
          boxShadow: '[0 0 0 4px {colors.primary/15}]',
        },
      },
    },
  },
  defaultVariants: {
    variant: 'filled',
  },
});
