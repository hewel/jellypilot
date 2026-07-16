import { css, cva } from '@styled-system/css';

export const trigger = css({
  position: 'relative',
});

export const triggerIcon = css({
  height: '5',
  width: '5',
});

export const statusDot = cva({
  base: {
    borderRadius: 'full',
    height: '2',
    position: 'absolute',
    right: '1',
    top: '1',
    width: '2',
  },
  variants: {
    status: {
      idle: {
        bg: 'outlineVariant',
      },
      offline: {
        bg: 'error',
        boxShadow: '[0 0 8px {colors.error}]',
      },
      paused: {
        bg: 'tertiary',
        boxShadow: '[0 0 8px {colors.tertiary}]',
      },
      playing: {
        bg: 'tertiary',
        boxShadow: '[0 0 8px {colors.tertiary}]',
      },
      unknown: {
        bg: 'outlineVariant',
      },
    },
  },
});

export const backdrop = css({
  bg: 'surfaceContainerLowest/70',
  backdropFilter: '[blur(4px)]',
  inset: '0',
  position: 'fixed',
  transitionDuration: '300',
  transitionProperty: '[backdrop-filter, background-color, opacity]',
  zIndex: '50',
  '&[data-state="closed"]': {
    opacity: '[0]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
  },
});

export const positioner = css({
  display: 'flex',
  inset: '0',
  justifyContent: 'flex-end',
  position: 'fixed',
  zIndex: '50',
});

export const content = css({
  backdropFilter: '[blur(24px)]',
  bg: 'surfaceContainerLow/60',
  borderLeftWidth: '1px',
  borderLeftStyle: 'solid',
  borderLeftColor: 'outlineVariant/30',
  borderTopLeftRadius: '4xl',
  borderBottomLeftRadius: '4xl',
  boxShadow: '2xl',
  display: 'flex',
  flexDirection: 'column',
  height: 'full',
  overflow: 'hidden',
  transitionDuration: '200',
  transitionProperty: '[opacity, transform]',
  transitionTimingFunction: 'standard',
  width: 'full',
  '&[data-state="closed"]': {
    opacity: '[0]',
    transform: '[translateX({spacing.3})]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
    transform: '[translateX(0)]',
  },
  sm: {
    width: '[28rem]',
  },
});

export const header = css({
  alignItems: 'center',
  borderBottomWidth: '1px',
  borderBottomStyle: 'solid',
  borderBottomColor: 'outlineVariant/20',
  display: 'flex',
  justifyContent: 'space-between',
  px: '5',
  py: '4',
});

export const title = css({
  color: 'onSurface',
  fontSize: '18',
  fontWeight: 'bold',
  lineHeight: '24',
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
  overflowY: 'auto',
  px: '5',
  py: '4',
});
