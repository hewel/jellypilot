import { css } from '@styled-system/css';

export const root = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLow/75',
  borderColor: 'outlineVariant',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: 'xl',
  position: 'sticky',
  top: '2',
  zIndex: '100',
});

export const inner = css({
  alignItems: 'center',
  display: 'flex',
  flexDirection: 'row',
  flexWrap: 'wrap',
  gap: '2',
  justifyContent: 'space-between',
  sm: {
    gap: '4',
  },
});

export const segments = css({
  borderRadius: 'xl',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '1',
  minWidth: '[0]',
  p: '1',
  position: 'relative',
});

export const indicator = css({
  bg: 'secondaryContainer',
  borderRadius: 'lg',
  bottom: '[var(--bottom)]',
  boxShadow: 'sm',
  height: '[var(--height)]',
  left: '[var(--left)]',
  position: 'absolute',
  right: '[var(--right)]',
  top: '[var(--top)]',
  width: '[var(--width)]',
});

export const item = css({
  alignItems: 'center',
  borderRadius: 'lg',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'inline-flex',
  fontSize: '14',
  fontWeight: 'bold',
  height: '10',
  justifyContent: 'center',
  lineHeight: '20',
  px: '4',
  position: 'relative',
  transitionProperty: '[color]',
  zIndex: '10',
  _hover: {
    color: 'onSurface',
  },
  '&[data-state="checked"]': {
    color: 'onSecondaryContainer',
  },
  '&[data-disabled]': {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
});

export const homeIcon = css({
  height: 'lg',
  width: 'lg',
});

export const srOnly = css({
  border: '[0]',
  clip: '[rect(0 0 0 0)]',
  height: '[1px]',
  margin: '[-1px]',
  overflow: 'hidden',
  padding: '[0]',
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: '[1px]',
});

export const portalTarget = css({
  display: 'flex',
  flexGrow: '[1]',
  justifyContent: 'flex-end',
  minWidth: '[0]',
});
