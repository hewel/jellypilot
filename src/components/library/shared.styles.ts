import { css } from '@styled-system/css';

export const statusCard = css({
  display: 'grid',
  gap: '5',
});

export const statusContent = css({
  alignItems: 'flex-start',
  display: 'flex',
  gap: '4',
});

export const statusIcon = css({
  alignItems: 'center',
  bg: 'tertiaryContainer/25',
  borderColor: 'tertiary/30',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'tertiary',
  display: 'flex',
  flexShrink: '[0]',
  height: '12',
  justifyContent: 'center',
  width: '12',
});

export const iconMd = css({
  height: '6',
  width: '6',
});

export const statusCopy = css({
  display: 'grid',
  gap: '2',
});

export const statusTitle = css({
  fontFamily: 'display',
  fontSize: '24',
  fontWeight: 'bold',
  lineHeight: '32',
});

export const statusDescription = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
});

export const row = css({
  display: 'grid',
  gap: '3',
});

export const rowTitle = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const videoGrid = css({
  display: 'grid',
  gap: '3',
  sm: {
    gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
  },
  xl: {
    gridTemplateColumns: '[repeat(4, minmax(0, 1fr))]',
  },
  '2xl': {
    gridTemplateColumns: '[repeat(6, minmax(0, 1fr))]',
  },
});

export const subtitleLink = css({
  color: 'secondary',
  textDecoration: 'none',
  textUnderlineOffset: '1',
  _hover: {
    textDecoration: 'underline',
  },
});

export const userDataControls = css({
  display: 'grid',
  gap: '2',
});

export const userDataActions = css({
  // Match DetailHero actions: column stack under sm, row wrap on sm+.
  alignItems: 'stretch',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
  '& > *': {
    boxSizing: 'border-box',
    maxWidth: '[100%]',
    minWidth: '[0]',
    width: 'full',
  },
  sm: {
    alignItems: 'center',
    flexDirection: 'row',
    flexWrap: 'wrap',
    '& > *': {
      width: 'auto',
    },
  },
});

export const pillButton = css({
  borderRadius: 'full',
  boxSizing: 'border-box',
  maxWidth: '[100%]',
});

export const favoriteSelected = css({
  borderColor: 'error/30',
});

export const playedSelected = css({
  borderColor: 'tertiary/30',
});

export const iconSm = css({
  height: '4',
  width: '4',
});

export const favoriteIcon = css({
  color: 'onSurfaceVariant',
});

export const favoriteIconSelected = css({
  color: 'error',
  fill: 'error',
});

export const playedIcon = css({
  color: 'onSurfaceVariant',
});

export const playedIconSelected = css({
  color: 'tertiary',
  fontWeight: 'bold',
});

export const spinIcon = css({
  animation: '[spin 1s linear infinite]',
  color: 'secondary',
  height: '4',
  width: '4',
});

export const errorText = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
});
