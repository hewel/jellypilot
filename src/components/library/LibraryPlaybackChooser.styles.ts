import { css } from '@styled-system/css';

export const content = css({
  maxWidth: '[42rem]',
  outline: 'none',
  position: 'relative',
  width: 'full',
});

export const card = css({
  bg: 'secondaryContainer/10',
  borderColor: 'secondary/40',
  display: 'grid',
  gap: '4',
});

export const eyebrow = css({
  color: 'secondary',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '8',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const title = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const fields = css({
  display: 'grid',
  gap: '4',
  sm: {
    gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
  },
});

export const actions = css({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
  justifyContent: 'flex-end',
});

export const closeButton = css({
  alignItems: 'center',
  bg: '[transparent]',
  borderColor: 'outline',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurface',
  cursor: 'pointer',
  display: 'inline-flex',
  fontSize: '14',
  fontWeight: 'bold',
  gap: '2',
  justifyContent: 'center',
  lineHeight: '20',
  minHeight: '11',
  px: '5',
  py: '3',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, color, transform]',
  userSelect: 'none',
  _hover: {
    bg: 'primary/5',
    borderColor: 'primary',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  _disabled: {
    opacity: '[0.5]',
    pointerEvents: 'none',
  },
});

export const icon = css({
  height: '4',
  width: '4',
});

export const playIcon = css({
  fill: '[currentColor]',
  height: '4',
  width: '4',
});
