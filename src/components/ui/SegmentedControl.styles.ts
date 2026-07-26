import { css } from '@styled-system/css';
import { focusRing, material, reducedMotionFeedback } from '~styles/recipes';

/** Narrow-layout contract values asserted by unit tests. */
export const segmentedControlNarrowLayout = {
  rootFlexWrap: 'wrap',
  rootMinWidth: '[0]',
  rootWidth: 'full',
  itemFlex: '[1 1 9rem]',
  itemMinWidth: '[0]',
  itemTextOverflow: 'ellipsis',
  itemTextWhiteSpace: 'nowrap',
} as const;

export const root = css({
  display: 'flex',
  flexWrap: segmentedControlNarrowLayout.rootFlexWrap,
  alignItems: 'center',
  gap: '2',
  minWidth: segmentedControlNarrowLayout.rootMinWidth,
  width: segmentedControlNarrowLayout.rootWidth,
});

export const label = css({
  color: 'onSurfaceVariant',
  flex: '[1 0 100%]',
  fontSize: '12',
  fontWeight: 'bold',
  letterSpacing: '5',
  lineHeight: '16',
  textTransform: 'uppercase',
  width: 'full',
});

/** The segment track is a recessed well holding the keys. */
export const trackStyles = css.raw(material.raw({ treatment: 'recessed' }), {
  display: 'flex',
  flexWrap: segmentedControlNarrowLayout.rootFlexWrap,
  alignItems: 'center',
  gap: '2',
  minWidth: segmentedControlNarrowLayout.rootMinWidth,
  position: 'relative',
  width: segmentedControlNarrowLayout.rootWidth,
  borderStyle: 'solid',
  borderWidth: '1px',
  borderRadius: '2xl',
  p: '1',
});

export const track = css(trackStyles);

/**
 * Sliding selection plate (pressed role). Ark drives its position through
 * `--top`/`--left`/`--width`/`--height`; the transition covers those plus
 * background/border so state changes stay smooth, and reduced motion makes
 * the plate land immediately.
 */
export const indicatorStyles = css.raw(material.raw({ treatment: 'pressed' }), {
  borderRadius: 'xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  height: '[var(--height)]',
  pointerEvents: 'none',
  position: 'absolute',
  top: '[var(--top)]',
  left: '[var(--left)]',
  width: '[var(--width)]',
  zIndex: '[0]',
  transitionProperty: '[top, left, width, height, background-color, border-color, box-shadow]',
  transitionDuration: '200',
  transitionTimingFunction: 'standard',
  _motionReduce: {
    transitionDuration: '100',
    transitionProperty: '[background-color, border-color, box-shadow]',
  },
});

export const indicator = css(indicatorStyles);

/** Unselected segments are keycaps; the checked state reads as pressed. */
export const item = (props: { checked: boolean }) =>
  css(
    material.raw({ treatment: props.checked ? 'pressed' : 'keycap' }),
    {
      alignItems: 'center',
      borderRadius: 'xl',
      borderStyle: 'solid',
      borderWidth: '1px',
      color: 'onSurface',
      cursor: 'pointer',
      display: 'inline-flex',
      flex: segmentedControlNarrowLayout.itemFlex,
      fontSize: '14',
      fontWeight: 'semibold',
      gap: '2',
      justifyContent: 'center',
      lineHeight: '20',
      minHeight: '[40px]',
      minWidth: segmentedControlNarrowLayout.itemMinWidth,
      outline: 'none',
      position: 'relative',
      px: '3',
      py: '2',
      textAlign: 'center',
      transitionDuration: '200',
      transitionProperty: '[background-color, border-color, box-shadow, color, opacity]',
      userSelect: 'none',
      zIndex: '[1]',
      _hover: {
        borderColor: 'materialEdgeNormal',
        bg: 'materialSurfaceKeyHover',
      },
      _focusVisible: focusRing,
      _disabled: {
        cursor: 'not-allowed',
        opacity: '[0.5]',
      },
      _motionReduce: reducedMotionFeedback,
    },
    props.checked ? { color: 'onPrimaryContainer' } : undefined,
  );

export const itemText = css({
  minWidth: '[0]',
  overflow: 'hidden',
  textOverflow: segmentedControlNarrowLayout.itemTextOverflow,
  whiteSpace: segmentedControlNarrowLayout.itemTextWhiteSpace,
});

export const itemControl = css({
  display: 'none',
});

export const hiddenInput = css({});
