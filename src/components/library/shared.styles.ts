import { css, cva } from '@styled-system/css';

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
  fontSize: '20',
  fontWeight: 'bold',
  lineHeight: '28',
  lg: {
    fontSize: '24',
    lineHeight: '32',
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

export const userDataControls = css({
  display: 'grid',
  gap: '2',
});

export const userDataActions = css({
  alignItems: 'flex-start',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  maxWidth: '[100%]',
  minWidth: '[0]',
  sm: {
    alignItems: 'flex-end',
    flexDirection: 'row',
    flexWrap: 'wrap',
  },
});

export const iconSm = css({
  height: '4',
  width: '4',
});

export const favoriteIcon = cva({
  base: { color: 'onSurfaceVariant' },
  variants: {
    selected: {
      true: { color: 'error', fill: 'error' },
    },
  },
});

export const playedIconSelected = css({
  color: 'tertiary',
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

export const iconAction = cva({
  base: {
    alignItems: 'center',
    appearance: 'none',
    aspectRatio: '[1 / 1]',
    bg: 'onSurface/10',
    borderColor: 'onSurface/15',
    borderRadius: 'full',
    borderStyle: 'solid',
    borderWidth: '1px',
    color: 'onSurfaceVariant',
    cursor: 'pointer',
    display: 'inline-flex',
    flex: 'none',
    height: '10',
    justifyContent: 'center',
    minWidth: '10',
    p: '0',
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
    _disabled: {
      cursor: 'not-allowed',
      opacity: '[0.5]',
    },
  },
  variants: {
    favorited: {
      true: { borderColor: 'error/30' },
    },
  },
});

export const menuContent = css({
  animation: '[menuIn 0.18s {easings.emphasized}]',
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  minWidth: '[13rem]',
  outline: 'none',
  p: '1_5',
  transformOrigin: 'top right',
  zIndex: '50',
});

export const menuItem = css({
  alignItems: 'center',
  borderRadius: 'lg',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'flex',
  fontSize: '14',
  lineHeight: '20',
  outline: 'none',
  px: '3',
  py: '2',
  transitionDuration: '150',
  transitionProperty: '[background-color, color]',
  _hover: {
    bg: 'surfaceContainerHigh',
    color: 'onSurface',
  },
  '&[data-highlighted]': {
    bg: 'surfaceContainerHigh',
    color: 'onSurface',
  },
  '&[data-disabled]': {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
});

export const menuItemRow = css({
  alignItems: 'center',
  display: 'inline-flex',
  gap: '2',
});

export const menuItemIcon = css({
  color: 'secondary',
  flex: 'none',
  height: '4',
  width: '4',
});

export const menuText = css({
  fontWeight: 'medium',
});
