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

export const positioner = css({
  position: 'fixed',
  display: 'flex',
  alignItems: addServiceDialogOverflowLayout.positionerAlignItems,
  justifyContent: 'center',
  overflowY: addServiceDialogOverflowLayout.positionerOverflowY,
  p: '4',
  zIndex: '60',
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

export const icon4_5 = css({
  height: 'lg',
  width: 'lg',
});
