import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]';

export const root = css({
  minWidth: '[0]',
});

export const section = css({
  display: 'grid',
  gap: '4',
});

export const header = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  sm: {
    alignItems: 'center',
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
});

export const title = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const count = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
});

export const grid = css({
  display: 'grid',
  gap: '3',
  gridTemplateColumns: '[repeat(auto-fill, minmax(min(100%, 160px), 1fr))]',
});

export const fade = css({
  animation: '[fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards]',
});

export const virtualGrid = css({
  animation: '[fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards]',
});

export const virtualCanvas = css({
  position: 'relative',
  width: 'full',
});

export const virtualRow = css({
  left: '0',
  position: 'absolute',
  top: '0',
  width: 'full',
});

export const loadMoreError = css({
  alignItems: 'center',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  pt: '2',
});

export const error = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  textAlign: 'center',
});

export const pillButton = css({
  borderRadius: 'full',
});

export const icon4 = css({
  height: '4',
  width: '4',
});

export const spin = css({
  animation: '[spin 1s linear infinite]',
});

export const sentinel = css({
  height: '[1px]',
  width: 'full',
});

export const menuTrigger = css({
  alignItems: 'center',
  border: '[0]',
  borderLeftColor: 'outlineVariant',
  borderLeftStyle: 'solid',
  borderLeftWidth: '1px',
  color: 'onSurface',
  display: 'flex',
  flex: 'none',
  height: '10',
  justifyContent: 'center',
  outline: 'none',
  px: '2',
  textAlign: 'left',
  transitionDuration: '200',
  transitionProperty: '[color]',
  width: '10',
  _hover: {
    color: 'secondary',
  },
  _disabled: {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
  '&[data-state="on"]': {
    bg: 'secondaryContainer/45',
    color: 'onSecondaryContainer',
  },
  sm: {
    height: '12',
    width: '12',
  },
});

export const menuContent = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderColor: 'outlineVariant',
  borderRadius: 'lg',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  maxHeight: '[15rem]',
  minWidth: '[12rem]',
  outline: 'none',
  overflowY: 'auto',
  p: '2',
  zIndex: '50',
});

export const menuLabel = css({
  fontSize: '12',
  fontWeight: 'bold',
  px: '3_5',
  py: '2',
});

export const menuItem = css({
  alignItems: 'center',
  borderRadius: 'xl',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'flex',
  fontSize: '14',
  justifyContent: 'space-between',
  lineHeight: '20',
  px: '3_5',
  py: '2_5',
  transitionProperty: '[background-color, color]',
  _hover: {
    bg: 'surfaceContainerHigh',
    color: 'onSurface',
  },
  '&[data-disabled]': {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
});

export const menuText = css({
  fontWeight: 'medium',
});

export const menuCheck = css({
  color: 'secondary',
  height: '4',
  width: '4',
});

export const separator = css({
  borderTopColor: 'outlineVariant/60',
  borderTopStyle: 'solid',
  borderTopWidth: '1px',
  my: '1',
});

export const controlsNav = css({
  alignItems: 'flex-end',
  display: 'flex',
  flexDirection: 'row',
  justifyContent: 'flex-end',
  mb: '6',
});

export const controlGroup = css({
  borderRadius: '2xl',
  display: 'flex',
  flexWrap: 'wrap',
  justifyContent: 'flex-end',
  minWidth: '[0]',
  overflow: 'hidden',
});

export const skeletonTitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'md',
  height: '7',
  width: '[8rem]',
});

export const skeletonCount = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/60',
  borderRadius: 'md',
  height: '4',
  width: '24',
});
