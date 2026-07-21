import { css } from '@styled-system/css';

export const trigger = css({
  lg: {
    justifyContent: 'flex-start',
  },
});

export const triggerIcon = css({
  height: '5',
  width: '5',
});

export const triggerLabel = css({
  display: 'none',
  fontSize: '14',
  lineHeight: '20',
  truncate: true,
  lg: {
    display: 'inline',
  },
});

export const backdrop = css({
  bg: 'surfaceContainerLowest/70',
  backdropFilter: '[blur(4px)]',
  inset: '0',
  position: 'fixed',
  transitionDuration: '300',
  transitionProperty: '[backdrop-filter, background-color, opacity]',
  zIndex: '100',
  '&[data-state="closed"]': {
    opacity: '[0]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
  },
});

export const positioner = css({
  display: 'flex',
  flexDirection: 'column',
  height: 'full',
  inset: '0',
  overflow: 'hidden',
  position: 'fixed',
  width: 'full',
  zIndex: '100',
});

export const content = css({
  backdropFilter: '[blur(24px)]',
  bg: 'surfaceContainerLow/60',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/30',
  display: 'flex',
  flexDirection: 'column',
  height: 'full',
  outline: 'none',
  overflow: 'hidden',
  transitionDuration: '200',
  transitionProperty: '[opacity, transform]',
  transitionTimingFunction: 'standard',
  width: 'full',
  '&[data-state="closed"]': {
    opacity: '[0]',
    transform: '[translateY({spacing.1})]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
    transform: '[translateY(0)]',
  },
});

export const header = css({
  alignItems: 'center',
  backdropFilter: '[blur(24px)]',
  bg: 'surfaceContainerLow/70',
  borderBottomWidth: '1px',
  borderBottomStyle: 'solid',
  borderBottomColor: 'outlineVariant/40',
  display: 'flex',
  gap: '3',
  justifyContent: 'space-between',
  px: '5',
  py: '4',
});

export const title = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const description = css({
  color: 'onSurfaceVariant/70',
  fontSize: '12',
  lineHeight: '16',
  mt: '0_5',
});

export const closeIcon = css({
  height: '5',
  width: '5',
});

export const body = css({
  flex: '[1]',
  minHeight: '[0]',
  overflowY: 'auto',
  px: '5',
  py: '4',
});
