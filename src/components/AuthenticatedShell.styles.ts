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
    mx: 'auto',
    pb: '8',
    pt: '2',
    px: '2_5',
    width: 'full',
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
    bg: 'surfaceContainerLow',
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
