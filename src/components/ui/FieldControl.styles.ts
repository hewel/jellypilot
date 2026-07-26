import { css } from '@styled-system/css';
import { focusRing, material, reducedMotionFeedback } from '~styles/recipes';

/**
 * Text inputs are recessed control wells in both variants; outlined keeps a
 * transparent body with a stronger edge. Focus uses the focusRing token.
 */
export const fieldControl = (props: { variant: 'filled' | 'outlined' }) =>
  css(
    material.raw({ treatment: 'recessed' }),
    {
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
      _focus: {
        borderColor: 'focusRing',
      },
      _focusVisible: focusRing,
      _motionReduce: reducedMotionFeedback,
    },
    props.variant === 'outlined' ? { bg: '[transparent]' } : undefined,
  );
