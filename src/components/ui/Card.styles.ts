import { css, cva } from '@styled-system/css';

export const card = cva({
  base: {
    position: 'relative',
    overflow: 'hidden',
    borderStyle: 'solid',
    borderWidth: '1px',
    transitionDuration: '300',
    transitionProperty: '[background-color, border-color, box-shadow]',
  },
  variants: {
    variant: {
      elevated: {
        bg: 'surfaceContainerLow/45',
        borderColor: 'primary/20',
        borderRadius: '4xl',
        p: '6',
        boxShadow: '2xl',
        _hover: {
          bg: 'surfaceContainerLow/60',
          borderColor: 'primary/35',
          boxShadow: '2xl',
        },
      },
      filled: {
        bg: 'surface/50',
        borderColor: 'outlineVariant/80',
        borderRadius: '2xl',
        boxShadow: 'xl',
        p: '4',
      },
      outlined: {
        bg: '[transparent]',
        borderColor: 'outlineVariant',
        borderRadius: '[1.75rem]',
        p: '6',
        _hover: {
          borderColor: 'outline/40',
        },
      },
    },
    padding: {
      default: {},
      none: {
        p: '0',
      },
    },
  },
  defaultVariants: {
    padding: 'default',
    variant: 'filled',
  },
});

export const tintOverlay = css({
  bg: 'surfaceTint/3',
  borderRadius: '[inherit]',
  inset: '[0]',
  pointerEvents: 'none',
  position: 'absolute',
});

export const content = css({
  position: 'relative',
  zIndex: '10',
});
