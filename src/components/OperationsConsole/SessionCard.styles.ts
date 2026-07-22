import { css } from '@styled-system/css';

export const header = css({
  display: 'flex',
  alignItems: 'flex-start',
  gap: '3',
});

export const cardIcon = css({
  color: 'error',
  height: '5',
  mt: '1',
  width: '5',
});

export const title = css({
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const description = css({
  mt: '1',
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
});

export const content = css({
  position: 'relative',
  overflow: 'hidden',
  p: '6',
  boxShadow: '2xl',
  backdropFilter: '[blur(24px)]',
  bg: 'surfaceContainerLow/45',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'error/30',
  borderRadius: '4xl',
  maxWidth: '[28rem]',
  transitionDuration: '300',
  transitionProperty: '[background-color, border-color, box-shadow, opacity, transform]',
  _hover: {
    bg: 'surfaceContainerLow/60',
    borderColor: 'primary/35',
  },
  '&[data-state="closed"]': {
    opacity: '[0]',
    transform: '[translateY(0.25rem)]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
    transform: '[translateY(0)]',
  },
});

export const dialogTitle = css({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  color: 'onSurface',
  fontSize: '22',
  lineHeight: '28',
  fontWeight: 'bold',
});

export const dialogIcon = css({
  width: '6',
  height: '6',
  color: 'error',
});

export const dialogDescription = css({
  mt: '3',
  color: 'onSurfaceVariant/90',
  fontSize: '14',
  lineHeight: '20',
});

export const actions = css({
  display: 'flex',
  flexDirection: 'column-reverse',
  gap: '3',
  mt: '6',
  sm: {
    flexDirection: 'row',
    justifyContent: 'flex-end',
  },
});

export const card = css({
  bg: 'errorContainer/5',
  borderColor: 'error/20',
  _hover: {
    borderColor: 'error/45',
  },
});

export const signOutButton = css({
  borderColor: 'error/55',
  color: 'error',
  mt: '5',
  width: 'full',
  _hover: {
    bg: 'error/10',
    borderColor: 'error',
  },
});

export const dangerButton = css({
  borderColor: 'error/60',
  color: 'error',
  _hover: {
    bg: 'error/10',
  },
});

export const icon4_5 = css({
  height: 'lg',
  width: 'lg',
});
