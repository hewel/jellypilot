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
  gap: '4',
  minWidth: '[0]',
});

export const rowHeader = css({
  alignItems: 'center',
  display: 'flex',
  gap: '4',
  justifyContent: 'space-between',
});

export const rowTitle = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '24',
  fontWeight: 'bold',
  lineHeight: '32',
  lg: {
    fontSize: '28',
    lineHeight: '40',
  },
});

export const rowDisclosure = css({
  flexShrink: '[0]',
  minHeight: '11',
});

const videoGridBase = {
  display: 'grid',
  minWidth: '[0]',
  rowGap: '6',
} as const;

export const videoGrid = {
  1: css({
    ...videoGridBase,
    gridTemplateColumns: '[minmax(0, 1fr)]',
  }),
  2: css({
    ...videoGridBase,
    gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
  }),
  3: css({
    ...videoGridBase,
    gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
  }),
  4: css({
    ...videoGridBase,
    gridTemplateColumns: '[repeat(4, minmax(0, 1fr))]',
  }),
  5: css({
    ...videoGridBase,
    gridTemplateColumns: '[repeat(5, minmax(0, 1fr))]',
  }),
  6: css({
    ...videoGridBase,
    gridTemplateColumns: '[repeat(6, minmax(0, 1fr))]',
  }),
  7: css({
    ...videoGridBase,
    gridTemplateColumns: '[repeat(7, minmax(0, 1fr))]',
  }),
};

export const videoGridGap = {
  poster: css({
    columnGap: '4',
  }),
  video: css({
    columnGap: '6',
  }),
};

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
  animation: '[spin 1s {easings.linear} infinite]',
  color: 'secondary',
  height: '4',
  width: '4',
});

export const errorText = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
});

export const pillRow = css({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const genre = css({
  borderColor: 'outlineVariant',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant/90',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '8',
  lineHeight: '16',
  px: '3',
  py: '1',
  textTransform: 'uppercase',
});
