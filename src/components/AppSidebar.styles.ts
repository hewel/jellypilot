import { css } from '@styled-system/css';

export const nav = css({
  bg: 'surfaceContainerLow',
  borderRightColor: 'outlineVariant/40',
  borderRightStyle: 'solid',
  borderRightWidth: '1px',
  display: 'flex',
  flexDirection: 'column',
  flexShrink: '0',
  gap: '1',
  height: '[100dvh]',
  overflowY: 'auto',
  position: 'sticky',
  px: '2',
  py: '2',
  top: '0',
  width: '[4rem]',
  lg: {
    width: '[16rem]',
  },
});

export const sectionLabel = css({
  color: 'onSurfaceVariant',
  display: 'none',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  px: '2',
  py: '1',
  textTransform: 'uppercase',
  lg: {
    display: 'block',
  },
});

export const list = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '1',
  listStyle: 'none',
  m: '0',
  p: '0',
});

export const item = css({
  alignItems: 'center',
  borderRadius: 'xl',
  color: 'onSurfaceVariant',
  display: 'flex',
  gap: '2',
  justifyContent: 'center',
  minHeight: '10',
  p: '2',
  position: 'relative',
  textDecoration: 'none',
  transitionDuration: '200',
  transitionProperty: '[background-color, color, transform]',
  transitionTimingFunction: 'standard',
  _active: {
    transform: '[scale(0.96)]',
  },
  _focusVisible: {
    outline: '[2px solid {colors.primary}]',
    outlineOffset: '[2px]',
  },
  _hover: {
    bg: 'surfaceContainerHigh',
  },
  '&::before': {
    bg: 'secondary',
    borderRadius: 'full',
    content: '""',
    height: '[60%]',
    left: '[-8px]',
    position: 'absolute',
    top: '[50%]',
    transform: '[translateY(-50%) scaleY(0)]',
    transitionDuration: '200',
    transitionProperty: '[transform]',
    transitionTimingFunction: 'standard',
    width: '[3px]',
  },
  '&[data-active]': {
    bg: 'secondaryContainer',
    color: 'onSecondaryContainer',
  },
  '&[data-active]::before': {
    transform: '[translateY(-50%) scaleY(1)]',
  },
  lg: {
    justifyContent: 'flex-start',
  },
});

export const itemIcon = css({
  flexShrink: '0',
  height: '5',
  width: '5',
});

export const itemThumb = css({
  borderRadius: 'md',
  flexShrink: '0',
  height: '6',
  objectFit: 'cover',
  outline: '[1px solid rgba(255, 255, 255, 0.1)]',
  outlineOffset: '[-1px]',
  width: '6',
});

export const itemLabel = css({
  display: 'none',
  fontSize: '14',
  lineHeight: '20',
  truncate: true,
  lg: {
    display: 'inline',
  },
});

export const footer = css({
  alignItems: 'stretch',
  borderTopColor: 'outlineVariant/40',
  borderTopStyle: 'solid',
  borderTopWidth: '1px',
  display: 'flex',
  flexDirection: 'column',
  gap: '1',
  mt: 'auto',
  pt: '2',
});
