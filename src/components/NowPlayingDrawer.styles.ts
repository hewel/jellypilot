import { css, cva } from '@styled-system/css';

/**
 * Narrow (360 stress) contracts for the Now Playing drawer shell.
 * Drawer is a flex child of the positioner — minWidth must be 0 so
 * intrinsic transport controls cannot expand past the viewport.
 */
export const nowPlayingDrawerNarrowLayout = {
  contentWidth: 'full',
  contentMaxWidth: '[100%]',
  contentMinWidth: '[0]',
  bodyMinWidth: '[0]',
  smWidth: '[28rem]',
} as const;

export const trigger = css({
  lg: {
    justifyContent: 'flex-start',
  },
});

export const triggerIcon = css({
  height: '5',
  width: '5',
});

export const triggerIconWrap = css({
  display: 'inline-flex',
  position: 'relative',
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
      },
      paused: {
        bg: 'tertiary',
      },
      playing: {
        bg: 'tertiary',
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
  minWidth: '[0]',
  position: 'fixed',
  width: 'full',
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
  boxSizing: 'border-box',
  display: 'flex',
  flexDirection: 'column',
  height: 'full',
  maxWidth: nowPlayingDrawerNarrowLayout.contentMaxWidth,
  minWidth: nowPlayingDrawerNarrowLayout.contentMinWidth,
  overflow: 'hidden',
  transitionDuration: '200',
  transitionProperty: '[opacity, transform]',
  transitionTimingFunction: 'standard',
  width: nowPlayingDrawerNarrowLayout.contentWidth,
  '&[data-state="closed"]': {
    opacity: '[0]',
    transform: '[translateX({spacing.3})]',
  },
  '&[data-state="open"]': {
    opacity: '[1]',
    transform: '[translateX(0)]',
  },
  sm: {
    width: nowPlayingDrawerNarrowLayout.smWidth,
  },
});

export const header = css({
  alignItems: 'center',
  borderBottomWidth: '1px',
  borderBottomStyle: 'solid',
  borderBottomColor: 'outlineVariant/20',
  boxSizing: 'border-box',
  display: 'flex',
  gap: '3',
  justifyContent: 'space-between',
  maxWidth: '[100%]',
  minWidth: '[0]',
  px: '4',
  py: '4',
  width: 'full',
  sm: {
    px: '5',
  },
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
  boxSizing: 'border-box',
  flex: '[1]',
  maxWidth: '[100%]',
  minWidth: nowPlayingDrawerNarrowLayout.bodyMinWidth,
  overflowX: 'auto',
  overflowY: 'auto',
  px: '4',
  py: '4',
  width: 'full',
  sm: {
    px: '5',
  },
});
