import { css } from '@styled-system/css';

export const row = css({
  alignItems: 'center',
  borderRadius: 'xl',
  color: 'onSurface',
  display: 'flex',
  gap: '3',
  minHeight: '[4.5rem]',
  minWidth: '[0]',
  px: '3',
  py: '2',
  textDecoration: 'none',
  transitionDuration: '200',
  transitionProperty: '[background-color]',
  _focusVisible: {
    outline: '[2px solid {colors.primary}]',
    outlineOffset: '[2px]',
  },
  _hover: {
    bg: 'surfaceContainerHigh/50',
  },
});

export const thumb = css({
  aspectRatio: '[16 / 9]',
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'lg',
  flex: 'none',
  overflow: 'hidden',
  position: 'relative',
  width: '[7rem]',
});

export const thumbImage = css({
  height: 'full',
  objectFit: 'cover',
  width: 'full',
});

export const thumbFallback = css({
  alignItems: 'center',
  color: 'onSurfaceVariant/60',
  display: 'flex',
  height: 'full',
  justifyContent: 'center',
  width: 'full',
});

export const fallbackIcon = css({
  height: '6',
  width: '6',
});

export const copy = css({
  display: 'flex',
  flex: '1',
  flexDirection: 'column',
  gap: '1',
  minWidth: '[0]',
});

export const title = css({
  fontSize: '14',
  fontWeight: 'semibold',
  lineHeight: '20',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const subtitle = css({
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const indicators = css({
  alignItems: 'center',
  display: 'flex',
  flex: 'none',
  gap: '3',
});

export const indicator = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'inline-flex',
  fontSize: '12',
  fontWeight: 'medium',
  gap: '1',
  lineHeight: '16',
  whiteSpace: 'nowrap',
});

export const playedIndicator = css({
  color: 'secondary',
});

export const favoriteIndicator = css({
  color: 'primary',
});
