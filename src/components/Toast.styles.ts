import { css, cva } from '@styled-system/css';

export const toast = cva({
  base: {
    alignItems: 'center',
    borderRadius: 'xl',
    boxShadow: '[0 10px 18px -6px {colors.materialDepthOverlay}]',
    display: 'flex',
    mb: '4',
    p: '4',
    width: 'full',
    borderWidth: '1px',
    borderStyle: 'solid',
    maxWidth: '[min(24rem, calc(100vw - 2rem))]',
    pointerEvents: 'auto',
    transitionDuration: '200',
    transitionProperty: '[filter, opacity, transform]',
    transitionTimingFunction: 'standard',
    _motionReduce: {
      animation: '[none]',
      filter: '[none]',
      transform: '[none]',
      transitionDuration: '100',
    },
  },
  variants: {
    level: {
      error: {
        bg: 'errorContainer',
        borderColor: 'error',
        color: 'onErrorContainer',
      },
      info: {
        bg: 'infoContainer',
        borderColor: 'info',
        color: 'onInfoContainer',
      },
      success: {
        bg: 'successContainer',
        borderColor: 'success',
        color: 'onSuccessContainer',
      },
      warning: {
        bg: 'warningContainer',
        borderColor: 'warning',
        color: 'onWarningContainer',
      },
    },
    state: {
      visible: {
        animation: '[fadeIn 200ms {easings.emphasized} forwards]',
        filter: '[blur(0)]',
        opacity: '[1]',
        transform: '[translateY(0)]',
      },
      exiting: {
        filter: '[blur(2px)]',
        opacity: '[0]',
        transform: '[translateY({spacing.1})]',
      },
    },
  },
});

export const iconWrap = css({
  alignItems: 'center',
  display: 'inline-flex',
  flexShrink: '[0]',
  justifyContent: 'center',
});

export const icon = cva({
  base: {
    height: '5',
    width: '5',
  },
  variants: {
    level: {
      error: { color: 'error' },
      info: { color: 'primary' },
      success: { color: 'tertiary' },
      warning: { color: 'warning' },
    },
  },
});

export const message = css({
  flexGrow: '[1]',
  fontSize: '14',
  fontWeight: 'normal',
  lineHeight: '20',
  ml: '3',
  color: 'onSurfaceVariant',
  overflowWrap: 'break-word',
});

export const closeButton = css({
  alignItems: 'center',
  borderRadius: 'full',
  display: 'inline-flex',
  height: '10',
  justifyContent: 'center',
  ml: 'auto',
  p: '1_5',
  width: '10',
  bg: '[transparent]',
  border: '0',
  color: '[currentColor]',
  cursor: 'pointer',
  mb: '[-0.375rem]',
  mr: '[-0.375rem]',
  mt: '[-0.375rem]',
  transitionDuration: '150',
  transitionProperty: '[background-color]',
  transitionTimingFunction: 'standard',
  _hover: {
    bg: 'onSurface/10',
  },
  _focusVisible: {
    outline: '[2px solid {colors.focusRing}]',
    outlineOffset: '[2px]',
  },
  _motionReduce: {
    transitionDuration: '100',
  },
});

export const closeIcon = css({
  height: '4',
  width: '4',
});
