import { css } from '@styled-system/css';
import { material } from '~styles/recipes';

/**
 * Card surfaces map to shared materials: filled → raised panel,
 * elevated → acrylic overlay pane, outlined → flat. Decoration lives in the
 * material treatments (edges, depth, pseudo-element sheen) — no DOM overlay.
 */
export const card = (props: {
  variant: 'filled' | 'elevated' | 'outlined';
  padding: 'default' | 'none';
}) =>
  css(
    material.raw({ treatment: variantTreatment[props.variant] }),
    {
      position: 'relative',
      overflow: 'hidden',
      borderStyle: 'solid',
      borderWidth: '1px',
      transitionDuration: '300',
      transitionProperty: '[background-color, border-color, box-shadow]',
      _motionReduce: {
        transitionDuration: '100',
      },
    },
    variantStyles[props.variant],
    props.padding === 'none' ? { p: '0' } : undefined,
  );

const variantTreatment = {
  filled: 'raised',
  elevated: 'acrylic',
  outlined: 'flat',
} as const;

const variantStyles = {
  elevated: {
    borderRadius: '4xl',
    p: '6',
    _hover: {
      borderColor: 'materialEdgeNormal',
    },
  },
  filled: {
    borderRadius: '2xl',
    p: '4',
  },
  outlined: {
    borderRadius: '[1.75rem]',
    p: '6',
    _hover: {
      borderColor: 'materialEdgeNormal',
    },
  },
} as const;

export const content = css({
  position: 'relative',
  zIndex: '10',
});
