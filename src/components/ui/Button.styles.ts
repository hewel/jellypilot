import { css } from '@styled-system/css';
import { focusRing, material, reducedMotionFeedback } from '~styles/recipes';

/**
 * Brand filled action. Primary keeps its own saturated fill (the brand
 * surface); every other variant maps to a shared material treatment:
 * secondary/tonal → keycap, outlined/text → flat, icon → flat.
 */
export const button = (props: {
  variant: 'primary' | 'secondary' | 'tonal' | 'outlined' | 'text';
  size: 'sm' | 'md' | 'lg';
}) =>
  css(
    material.raw({
      treatment: props.variant === 'primary' ? undefined : variantTreatment[props.variant],
    }),
    {
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
      _motionReduce: reducedMotionFeedback,
    },
    sizeStyles[props.size],
    variantStyles[props.variant],
  );

const variantTreatment = {
  secondary: 'keycap',
  tonal: 'keycap',
  outlined: 'flat',
  text: 'flat',
} as const;

const sizeStyles = {
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
} as const;

const variantStyles = {
  primary: {
    bg: 'primary',
    color: 'onPrimary',
    _hover: {
      filter: '[brightness(1.1)]',
    },
    _active: {
      transform: '[translateY(0) scale(0.96)]',
    },
  },
  secondary: {
    borderWidth: '1px',
    borderStyle: 'solid',
    color: 'onSecondaryContainer',
    _hover: {
      bg: 'materialSurfaceKeyHover',
    },
    _active: {
      transform: '[translateY(0) scale(0.96)]',
    },
  },
  tonal: {
    borderWidth: '1px',
    borderStyle: 'solid',
    color: 'onSecondaryContainer',
    _hover: {
      bg: 'materialSurfaceKeyHover',
    },
    _active: {
      transform: '[translateY(0) scale(0.96)]',
    },
  },
  outlined: {
    borderWidth: '1px',
    borderStyle: 'solid',
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
    color: 'secondary',
    _hover: {
      bg: 'secondary/10',
    },
    _active: {
      transform: '[scale(0.96)]',
    },
  },
} as const;

/** Icon/row controls keep their existing square/row layout on a flat material. */
export const iconButton = (props: { size: 'sm' | 'md' | 'lg' | 'row' }) =>
  css(
    material.raw({ treatment: 'flat' }),
    {
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      color: 'onSurfaceVariant',
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
      _motionReduce: reducedMotionFeedback,
    },
    iconSizeStyles[props.size],
  );

const iconSizeStyles = {
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
  /*
   * Full-width row chrome for rail/footer triggers. The size block wins over
   * base padding and hover via merge order; owners must not re-declare
   * them in a local class (atomic emission order would not guarantee a win).
   */
  row: {
    borderRadius: 'xl',
    gap: '2',
    height: 'auto',
    minHeight: '10',
    minWidth: '[0]',
    padding: '2',
    width: 'full',
    _hover: {
      bg: 'surfaceContainerHigh',
      color: 'onSurfaceVariant',
    },
  },
} as const;

export const buttonIcon = css({
  alignItems: 'center',
  display: 'inline-flex',
  flexShrink: '[0]',
  height: '[1lh]',
  justifyContent: 'center',
  width: '[1lh]',
});
