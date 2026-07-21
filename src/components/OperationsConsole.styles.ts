import { css } from '@styled-system/css';

export const stack = css({
  display: 'grid',
  gap: '6',
});

export const addServiceDialogOverflowLayout = {
  contentMarginBlock: 'auto',
  positionerAlignItems: 'flex-start',
  positionerOverflowY: 'auto',
} as const;

export const backdrop = css({
  backdropFilter: '[blur(4px)]',
  bg: '[rgb(0 0 0 / 0.7)]',
  inset: '0',
  position: 'fixed',
  transitionDuration: '300',
  transitionProperty: '[backdrop-filter, background-color, opacity]',
  zIndex: '60',
  '&[data-state="closed"]': { opacity: '[0]' },
  '&[data-state="open"]': { opacity: '[1]' },
});

export const positioner = css({
  position: 'fixed',
  display: 'flex',
  alignItems: addServiceDialogOverflowLayout.positionerAlignItems,
  justifyContent: 'center',
  overflowY: addServiceDialogOverflowLayout.positionerOverflowY,
  p: '4',
  zIndex: '60',
});

export const positionerFill = css({
  inset: '0',
});

export const content = css({
  my: addServiceDialogOverflowLayout.contentMarginBlock,
  maxWidth: '[48rem]',
  outline: 'none',
  position: 'relative',
  width: 'full',
});

export const closeButton = css({
  bg: 'surfaceContainerHigh/80',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  boxShadow: 'lg',
  color: 'onSurfaceVariant',
  position: 'absolute',
  right: '4',
  top: '4',
  zIndex: '10',
  _hover: {
    borderColor: 'secondary',
    color: 'secondary',
  },
});

export const srOnly = css({
  border: 0,
  clip: '[rect(0 0 0 0)]',
  height: 'px',
  margin: '[-1px]',
  overflow: 'hidden',
  padding: '0',
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: 'px',
});

export const icon4_5 = css({
  height: '[1.125rem]',
  width: '[1.125rem]',
});
