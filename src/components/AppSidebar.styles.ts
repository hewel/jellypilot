import { css, cva } from '@styled-system/css';

export const appSidebarViewportLayout = {
  height: '[100dvh]',
  left: '0',
  overflowY: 'auto',
  position: 'fixed',
  top: '0',
} as const;

export const nav = cva({
  base: {
    ...appSidebarViewportLayout,
    '&[data-wiping]': {
      backdropFilter: '[none]',
      bg: '[transparent]',
      borderRightColor: '[transparent]',
    },
    backdropFilter: '[blur(20px)]',
    bg: 'surfaceContainerLow/70',
    borderRightColor: 'outlineVariant/40',
    borderRightStyle: 'solid',
    borderRightWidth: '1px',
    display: 'flex',
    flexDirection: 'column',
    flexShrink: '0',
    gap: '1',
    px: '2',
    py: '2',
    width: '[4rem]',
    zIndex: '40',
    lg: {
      width: '[16rem]',
    },
  },
  variants: {
    collapsed: {
      true: {
        lg: {
          width: '[4.5rem]',
        },
      },
    },
  },
});

export const sectionLabel = cva({
  base: {
    _motionReduce: {
      animation: '[none]',
    },
    animation: '[sidebarLabelIn 150ms {easings.standard} both]',
    color: 'onSurfaceVariant',
    display: 'none',
    fontSize: '11',
    fontWeight: 'bold',
    letterSpacing: '8',
    px: '2',
    py: '1',
    textTransform: 'uppercase',
    lg: {
      display: 'block',
    },
  },
  variants: {
    collapsed: {
      true: {
        lg: {
          display: 'none',
        },
      },
    },
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

export const item = cva({
  base: {
    alignItems: 'center',
    borderRadius: 'xl',
    color: 'onSurfaceVariant',
    display: 'flex',
    gap: '2',
    justifyContent: 'center',
    minHeight: '10',
    p: '2',
    textDecoration: 'none',
    transitionDuration: '200',
    transitionProperty: '[background-color, color]',
    transitionTimingFunction: 'standard',
    _focusVisible: {
      outline: '[2px solid {colors.primary}]',
      outlineOffset: '[2px]',
    },
    _hover: {
      bg: 'surfaceContainerHigh/55',
    },
    '&[data-active]': {
      bg: 'secondaryContainer',
      color: 'onSecondaryContainer',
    },
    lg: {
      justifyContent: 'flex-start',
    },
  },
  variants: {
    collapsed: {
      true: {
        lg: {
          justifyContent: 'center',
        },
      },
    },
  },
});

export const itemIcon = css({
  flexShrink: '0',
  height: '5',
  width: '5',
});

/* Fixed 24px leading slot so icon-only rows align with artwork rows. */
export const itemIconSlot = css({
  alignItems: 'center',
  display: 'inline-flex',
  flexShrink: '0',
  height: '6',
  justifyContent: 'center',
  width: '6',
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

export const itemLabel = cva({
  base: {
    _motionReduce: {
      animation: '[none]',
    },
    animation: '[sidebarLabelIn 150ms {easings.standard} both]',
    display: 'none',
    fontSize: '14',
    lineHeight: '20',
    truncate: true,
    lg: {
      display: 'inline',
    },
  },
  variants: {
    collapsed: {
      true: {
        lg: {
          display: 'none',
        },
      },
    },
  },
});

export const header = css({
  alignItems: 'stretch',
  borderBottomColor: 'outlineVariant/40',
  borderBottomStyle: 'solid',
  borderBottomWidth: '1px',
  display: 'flex',
  flexDirection: 'column',
  gap: '1',
  mb: '1',
  pb: '2',
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

export const collapseToggle = cva({
  base: {
    lg: {
      justifyContent: 'flex-start',
    },
  },
  variants: {
    collapsed: {
      true: {
        lg: {
          justifyContent: 'center',
        },
      },
    },
  },
});

export const collapseToggleIcon = css({
  _motionReduce: {
    animation: '[none]',
  },
  animation: '[iconSwapIn 200ms {easings.standard} both]',
  height: '5',
  width: '5',
});

export const collapseToggleLabel = cva({
  base: {
    _motionReduce: {
      animation: '[none]',
    },
    animation: '[sidebarLabelIn 150ms {easings.standard} both]',
    display: 'none',
    fontSize: '14',
    lineHeight: '20',
    truncate: true,
    lg: {
      display: 'inline',
    },
  },
  variants: {
    collapsed: {
      true: {
        lg: {
          display: 'none',
        },
      },
    },
  },
});
