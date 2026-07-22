import { css, cva } from '@styled-system/css';

export const toast = cva({
  base: {
    alignItems: 'center',
    borderRadius: 'xl',
    boxShadow: 'md',
    display: 'flex',
    mb: '4',
    p: '4',
    width: 'full',
    backdropFilter: '[blur(12px)]',
    borderWidth: '1px',
    borderStyle: 'solid',
    maxWidth: '[24rem]',
    pointerEvents: 'auto',
    transitionDuration: '200',
    transitionProperty: '[filter, opacity, transform]',
    transitionTimingFunction: 'standard',
  },
  variants: {
    level: {
      error: {
        bg: 'errorContainer/85',
        borderColor: 'error/25',
        boxShadow: '[var(--shadows-2xl), 0 25px 50px -12px {colors.error/10}]',
        color: 'onErrorContainer',
      },
      info: {
        bg: 'surfaceContainerHigh/90',
        borderColor: 'outlineVariant/60',
        boxShadow: '2xl',
        color: 'onSurface',
      },
      success: {
        bg: 'surfaceContainerHigh/90',
        borderColor: 'tertiary/20',
        boxShadow: '[var(--shadows-2xl), 0 25px 50px -12px {colors.tertiary/5}]',
        color: 'onSurface',
      },
      warning: {
        bg: 'warningContainer/85',
        borderColor: 'warning/25',
        boxShadow: '[var(--shadows-2xl), 0 25px 50px -12px {colors.warning/10}]',
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
});

export const closeIcon = css({
  height: '4',
  width: '4',
});
