import { css, cva, cx } from '@styled-system/css';

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
        backdropFilter: '[blur(24px)]',
        bg: 'surfaceContainerLow/45',
        borderColor: 'primary/20',
        borderRadius: '4xl',
        p: '6',
        backgroundImage:
          '[linear-gradient(135deg, rgba(28, 32, 48, 0.45) 0%, rgba(17, 19, 28, 0.65) 100%)]',
        boxShadow: '[0 25px 50px -12px rgba(0, 0, 0, 0.6), 0 0 40px -10px rgba(79, 70, 229, 0.12)]',
        _hover: {
          bg: 'surfaceContainerLow/60',
          borderColor: 'primary/35',
          boxShadow:
            '[0 30px 60px -12px rgba(0, 0, 0, 0.75), 0 0 50px -5px rgba(79, 70, 229, 0.22)]',
        },
      },
      filled: {
        backdropFilter: '[blur(12px)]',
        bg: 'surface/50',
        borderColor: 'outlineVariant/80',
        borderRadius: '2xl',
        boxShadow: 'xl',
        p: '4',
        backgroundImage:
          '[linear-gradient(135deg, rgba(21, 24, 35, 0.5) 0%, rgba(11, 13, 20, 0.7) 100%)]',
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

export { cx };
