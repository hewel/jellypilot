import { css, cva } from '@styled-system/css';

const focusRing = {
  outline: '[2px solid {colors.primary}]',
  outlineOffset: '[2px]',
} as const;

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

export const track = css({
  display: 'flex',
  flexWrap: segmentedControlNarrowLayout.rootFlexWrap,
  alignItems: 'center',
  gap: '2',
  minWidth: segmentedControlNarrowLayout.rootMinWidth,
  position: 'relative',
  width: segmentedControlNarrowLayout.rootWidth,
});

export const indicator = css({
  backgroundColor: 'primaryContainer/35',
  borderColor: 'primary',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  height: '[var(--height)]',
  pointerEvents: 'none',
  position: 'absolute',
  top: '[var(--top)]',
  width: '[var(--width)]',
  zIndex: '[0]',
});

export const item = cva({
  base: {
    alignItems: 'center',
    borderColor: 'outlineVariant',
    borderRadius: '2xl',
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
      borderColor: 'primary/50',
      bg: 'surfaceContainerHigh/60',
    },
    _focusVisible: focusRing,
    _disabled: {
      cursor: 'not-allowed',
      opacity: '[0.5]',
    },
    '&[data-state=checked]': {
      bg: 'primaryContainer/35',
      borderColor: 'primary',
      color: 'onPrimaryContainer',
    },
  },
});

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
