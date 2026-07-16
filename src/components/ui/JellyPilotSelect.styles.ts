import { css, cva, cx } from '@styled-system/css';

export const label = cva({
  base: {
    display: 'block',
    color: 'onSurfaceVariant',
    fontSize: '12',
    fontWeight: 'bold',
    lineHeight: '16',
    letterSpacing: '[0.05em]',
    textTransform: 'uppercase',
  },
  variants: {
    size: {
      compact: { mb: '2' },
      standard: { mb: '1_5' },
    },
  },
});

export const control = css({
  display: 'flex',
  width: 'full',
  alignItems: 'center',
});

export const trigger = cva({
  base: {
    display: 'flex',
    width: 'full',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: '2',
    color: 'onSurface',
    borderStyle: 'solid',
    borderWidth: '1px',
    outline: 'none',
    textAlign: 'left',
    transitionDuration: '200',
    transitionProperty: '[background-color, border-color, box-shadow]',
    _hover: {
      borderColor: 'secondary/50',
    },
    _focus: {
      borderColor: 'secondary',
    },
    _disabled: {
      cursor: 'not-allowed',
      opacity: '[0.5]',
    },
  },
  variants: {
    size: {
      compact: {
        bg: 'surfaceContainerLow',
        borderColor: 'outlineVariant',
        borderRadius: 'lg',
        height: '12',
        px: '3',
        _focus: {
          boxShadow: '[0 0 0 2px {colors.secondary/25}]',
        },
      },
      standard: {
        bg: 'surfaceContainerHighest/30',
        borderColor: 'outlineVariant/80',
        borderRadius: '2xl',
        height: '14',
        px: '4',
        _focus: {
          boxShadow: '[0 0 0 4px {colors.secondary/15}]',
        },
      },
    },
  },
});

export const content = css({
  overflowY: 'auto',
  borderRadius: 'lg',
  p: '2',
  boxShadow: '2xl',
  zIndex: '50',
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  maxHeight: '[15rem]',
});

export const item = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  borderRadius: 'xl',
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  cursor: 'pointer',
  py: '2_5',
  px: '3_5',
  transitionDuration: '200',
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

export const itemText = css({
  fontWeight: 'medium',
});

export const valueText = css({
  minWidth: '[0]',
  color: 'onSurface',
  fontSize: '14',
  fontWeight: 'medium',
  lineHeight: '20',
});

export const indicator = css({});

export const indicatorIcon = css({
  color: 'onSurfaceVariant',
  height: '4',
  width: '4',
  opacity: '[0.7]',
  transitionDuration: '200',
  transitionProperty: '[transform]',
  '[data-state=open] &': {
    transform: '[rotate(180deg)]',
  },
});

export const truncate = css({
  minWidth: '[0]',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export { cx };
