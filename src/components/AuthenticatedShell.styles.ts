import { css, cva } from '@styled-system/css';

export const authenticatedShellLayout = {
  color: 'onSurface',
  display: 'grid',
  gridTemplateColumns: '[4rem minmax(0, 1fr)]',
  minHeight: '[100dvh]',
  overflowX: 'clip',
  position: 'relative',
  lg: {
    gridTemplateColumns: '[16rem minmax(0, 1fr)]',
  },
} as const;

export const authenticatedShellCollapsedTrackLayout = {
  lg: {
    gridTemplateColumns: '[4.5rem minmax(0, 1fr)]',
  },
} as const;

/** Layer 0 ambient contract: pinned to the window, inert, clipped, below main. */
export const authenticatedShellAmbientLayout = {
  height: '[100dvh]',
  left: '0',
  overflow: 'hidden',
  pointerEvents: 'none',
  position: 'fixed',
  top: '0',
  width: 'full',
  zIndex: '0',
} as const;

/* Shared ambient layer: one static gradient blob spanning sidebar and main.
 * Hover on the sidebar or browse toolbar is promoted via :has() so the core
 * lifts uniformly. */
export const ambient = css(authenticatedShellAmbientLayout);

export const ambientGlow = css({
  borderRadius: 'full',
  height: '[55vmax]',
  left: '[40vw]',
  position: 'absolute',
  top: '[28vh]',
  transform: '[translate(-50%, -50%)]',
  width: '[55vmax]',
});

export const ambientCore = css({
  backgroundImage:
    '[radial-gradient(closest-side, {colors.primary/16}, {colors.secondary/7} 48%, transparent 72%)]',
  borderRadius: 'full',
  height: 'full',
  opacity: '[0.75]',
  transitionDuration: '300',
  transitionProperty: '[opacity, transform]',
  transitionTimingFunction: 'standard',
  width: 'full',
  _motionReduce: {
    transition: '[none]',
  },
  '[data-shell]:has([data-sidebar]:hover) &, [data-shell]:has([data-toolbar]:hover) &': {
    opacity: '[1]',
    transform: '[scale(1.06)]',
  },
});

export const shell = cva({
  base: authenticatedShellLayout,
  variants: {
    collapsed: {
      true: authenticatedShellCollapsedTrackLayout,
    },
  },
});

export const main = cva({
  base: {
    color: 'onSurface',
    display: 'flex',
    flex: '1',
    flexDirection: 'column',
    gridColumn: '2',
    minWidth: '[0]',
    position: 'relative',
    pl: '2',
    width: 'full',
    zIndex: '0',
  },
  variants: {
    glide: {
      expand: {
        animation: '[sidebarGlideExpand 200ms {easings.emphasized}]',
      },
      collapse: {
        animation: '[sidebarGlideCollapse 200ms {easings.emphasized}]',
      },
    },
  },
});

export const enter = css({
  animation: '[fadeIn 300ms {easings.emphasized} forwards]',
  display: 'flex',
  flex: '1',
  flexDirection: 'column',
  minWidth: '[0]',
});

/* Solid-color stand-in for the sidebar background while its layout width
 * snaps. Lives above main and below the nav so the rail icons and labels
 * stay visible while the panel sweeps. Transform-only: scaleX never touches
 * layout. 0.28125 = 4.5rem collapsed rail / 16rem expanded sidebar. */
export const sidebarWipe = cva({
  base: {
    backdropFilter: '[blur(20px)]',
    bg: 'surfaceContainerLow/70',
    borderRightColor: 'outlineVariant/40',
    borderRightStyle: 'solid',
    borderRightWidth: '1px',
    height: '[100dvh]',
    left: '0',
    pointerEvents: 'none',
    position: 'fixed',
    top: '0',
    transformOrigin: '[left center]',
    width: '[16rem]',
    zIndex: '10',
  },
  variants: {
    direction: {
      expand: {
        animation: '[sidebarWipeExpand 200ms {easings.emphasized}]',
      },
      collapse: {
        animation: '[sidebarWipeCollapse 200ms {easings.emphasized}]',
      },
    },
  },
});
