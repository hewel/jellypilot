import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

export const section = css({
  display: 'grid',
  gap: '4',
  minWidth: '[0]',
  px: '4',
  width: 'full',
  lg: {
    px: '8',
  },
});

export const header = css({
  alignItems: 'center',
  display: 'flex',
  gap: '3',
  justifyContent: 'space-between',
  minWidth: '[0]',
});

export const heading = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '22',
  fontWeight: 'bold',
  letterSpacing: '[-0.01em]',
  lineHeight: '28',
  m: '0',
});

export const controls = css({
  alignItems: 'center',
  display: 'flex',
  gap: '2',
});

export const arrow = css({
  alignItems: 'center',
  appearance: 'none',
  bg: 'onSurface/10',
  borderColor: 'onSurface/15',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'inline-flex',
  height: '10',
  justifyContent: 'center',
  transitionDuration: '150',
  transitionProperty: '[background-color, color]',
  width: '10',
  _hover: {
    bg: 'onSurface/15',
    color: 'onSurface',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
  '&[data-disabled]': {
    cursor: 'not-allowed',
    opacity: '[0.4]',
  },
});

export const arrowIcon = css({
  height: '4',
  width: '4',
});

export const viewport = css({
  minWidth: '[0]',
  overflow: 'hidden',
});

export const itemGroup = css({
  display: 'flex',
  gap: '4',
});

export const item = css({
  flex: 'none',
  minWidth: '[0]',
});

export const card = css({
  display: 'grid',
  gap: '2',
  minWidth: '[0]',
  textDecoration: 'none',
});

export const poster = css({
  aspectRatio: '[2 / 3]',
  borderRadius: 'xl',
  boxShadow: 'md',
  objectFit: 'cover',
  outline: '[1px solid rgb(255 255 255 / 0.08)]',
  overflow: 'hidden',
  transitionDuration: '200',
  transitionProperty: '[opacity]',
  width: 'full',
});

export const posterFallback = css({
  alignItems: 'center',
  bg: 'surfaceContainerHigh',
  color: 'onSurface/20',
  display: 'flex',
  fontFamily: 'display',
  fontSize: '[2rem]',
  fontWeight: 'extrabold',
  height: 'full',
  justifyContent: 'center',
  width: 'full',
});

export const cardTitle = css({
  color: 'onSurface',
  fontSize: '14',
  fontWeight: 'semibold',
  lineHeight: '20',
  m: '0',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const cardMeta = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '12',
  gap: '1_5',
  lineHeight: '16',
});

export const cardMetaSeparator = css({
  color: 'outline/40',
});

export const cardPlayed = css({
  color: 'tertiary',
  fontWeight: 'semibold',
});

export const cardFavorite = css({
  color: 'error',
  fontWeight: 'semibold',
});

export const statusWrap = css({
  display: 'grid',
  gap: '3',
  justifyItems: 'start',
});

export const retryButton = css({
  borderRadius: 'full',
});

export const skeletonSection = css({
  display: 'grid',
  gap: '4',
  minWidth: '[0]',
  px: '4',
  width: 'full',
  lg: {
    px: '8',
  },
});

export const skeletonHeading = css({
  animation: pulse,
  bg: 'onSurface/15',
  borderRadius: 'md',
  height: '7',
  width: '[10rem]',
});

export const skeletonRow = css({
  display: 'flex',
  gap: '4',
});

export const skeletonCard = css({
  animation: pulse,
  aspectRatio: '[2 / 3]',
  bg: 'onSurface/10',
  borderRadius: 'xl',
  flex: 'none',
  width: '[9rem]',
});
