import { css } from '@styled-system/css';
import { focusRing, material, reducedMotionFeedback } from '~styles/recipes';

export const label = (props: { size: 'compact' | 'standard' }) =>
  css(
    {
      display: 'block',
      color: 'onSurfaceVariant',
      fontSize: '12',
      fontWeight: 'bold',
      lineHeight: '16',
      letterSpacing: '5',
      textTransform: 'uppercase',
    },
    props.size === 'compact' ? { mb: '2' } : { mb: '1_5' },
  );

export const control = css({
  display: 'flex',
  width: 'full',
  alignItems: 'center',
});

/** Select triggers are recessed control wells; focus uses the focusRing token. */
export const trigger = (props: { size: 'compact' | 'standard' }) =>
  css(
    material.raw({ treatment: 'recessed' }),
    {
      display: 'flex',
      width: 'full',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '2',
      color: 'onSurface',
      borderStyle: 'solid',
      borderWidth: '1px',
      outline: 'none',
      textAlign: 'left',
      transitionDuration: '200',
      transitionProperty: '[background-color, border-color, box-shadow]',
      _hover: {
        borderColor: 'secondary/50',
      },
      _focus: {
        borderColor: 'focusRing',
      },
      _focusVisible: focusRing,
      _disabled: {
        cursor: 'not-allowed',
        opacity: '[0.5]',
      },
      _motionReduce: reducedMotionFeedback,
    },
    triggerSizeStyles[props.size],
  );

const triggerSizeStyles = {
  compact: {
    borderRadius: 'lg',
    height: '12',
    px: '3',
  },
  standard: {
    borderRadius: '2xl',
    height: '14',
    px: '4',
  },
} as const;

/** Dropdown content is a glass overlay pane (opaque fallback is the contract). */
export const content = css(material.raw({ treatment: 'glass' }), {
  overflowY: 'auto',
  borderRadius: 'lg',
  p: '2',
  zIndex: '50',
  borderStyle: 'solid',
  borderWidth: '1px',
  maxHeight: '[15rem]',
});

export const item = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  borderRadius: 'xl',
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  cursor: 'pointer',
  py: '2_5',
  px: '3_5',
  transitionDuration: '200',
  transitionProperty: '[background-color, color]',
  _hover: {
    bg: 'surfaceContainerHigh',
    color: 'onSurface',
  },
  '&[data-disabled]': {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
  _motionReduce: {
    transitionDuration: '100',
  },
});

export const itemText = css({
  fontWeight: 'medium',
});

export const valueText = css({
  minWidth: '[0]',
  color: 'onSurface',
  fontSize: '14',
  fontWeight: 'medium',
  lineHeight: '20',
});

export const indicator = css({});

export const indicatorIcon = css({
  color: 'onSurfaceVariant',
  height: '4',
  width: '4',
  opacity: '[0.7]',
  transitionDuration: '200',
  transitionProperty: '[transform]',
  '[data-state=open] &': {
    transform: '[rotate(180deg)]',
  },
  _motionReduce: reducedMotionFeedback,
});

export const truncate = css({
  minWidth: '[0]',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});
