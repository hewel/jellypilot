import { css, cva } from '@styled-system/css';

const focusRing = {
  outline: '[2px solid {colors.primary}]',
  outlineOffset: '[2px]',
} as const;

export const root = css({
  backgroundColor: 'surface',
  color: 'onSurface',
  display: 'grid',
  height: '[100dvh]',
  minHeight: '[28rem]',
  overflow: 'hidden',
  position: 'relative',
  width: 'full',
});

export const video = css({
  backgroundColor: '[#000]',
  gridArea: '[1 / 1]',
  height: 'full',
  objectFit: 'contain',
  width: 'full',
});

export const scrim = css({
  backgroundImage:
    '[linear-gradient(to bottom, rgb(5 6 10 / 0.72), transparent 24%, transparent 62%, rgb(5 6 10 / 0.94))]',
  gridArea: '[1 / 1]',
  pointerEvents: 'none',
});

export const titleBar = css({
  alignItems: 'start',
  display: 'grid',
  gap: '1',
  gridArea: '[1 / 1]',
  maxWidth: '[min(100%, 54rem)]',
  padding: '5',
  pointerEvents: 'none',
  placeSelf: 'start',
  zIndex: '[1]',
  sm: {
    padding: '6',
  },
});

export const eyebrow = css({
  color: 'onSurfaceVariant',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '8',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const title = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '20',
  fontWeight: 'bold',
  lineHeight: '28',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  sm: {
    fontSize: '24',
    lineHeight: '32',
  },
});

export const subtitle = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const stateOverlay = css({
  alignItems: 'center',
  display: 'grid',
  gridArea: '[1 / 1]',
  justifyItems: 'center',
  padding: '5',
  placeSelf: 'center',
  textAlign: 'center',
  width: 'full',
  zIndex: '[4]',
});

export const statePanel = css({
  backdropFilter: '[blur(16px)]',
  backgroundColor: 'surfaceContainer/90',
  borderColor: 'outlineVariant',
  borderRadius: '3xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  display: 'grid',
  gap: '3',
  justifyItems: 'center',
  maxWidth: '[30rem]',
  padding: '6',
  width: 'full',
});

export const stateIcon = css({
  color: 'secondary',
  height: '8',
  width: '8',
});

export const errorIcon = css({
  color: 'error',
  height: '8',
  width: '8',
});

export const stateTitle = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '20',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const stateMessage = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  maxWidth: '[26rem]',
});

export const stateActions = css({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
  justifyContent: 'center',
  paddingTop: '1',
});

export const loadingIndicator = css({
  animation: '[spin 1s {easings.linear} infinite]',
  color: 'secondary',
  height: '8',
  width: '8',
  _motionReduce: {
    animation: '[none]',
  },
});

export const controls = css({
  alignSelf: 'end',
  backgroundImage: '[linear-gradient(to top, rgb(5 6 10 / 0.96), rgb(5 6 10 / 0.72), transparent)]',
  display: 'grid',
  gap: '3',
  gridArea: '[1 / 1]',
  padding: '5',
  paddingTop: '12',
  width: 'full',
  zIndex: '[3]',
  sm: {
    padding: '6',
  },
});

export const timelineRow = css({
  alignItems: 'center',
  display: 'grid',
  gap: '3',
  gridTemplateColumns: '[minmax(3.5rem, auto) minmax(0, 1fr) minmax(3.5rem, auto)]',
});

export const time = css({
  color: 'onSurfaceVariant',
  fontFamily: 'mono',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
  textAlign: 'center',
});

export const range = css({
  accentColor: 'primary',
  cursor: 'pointer',
  height: '6',
  minWidth: '[0]',
  width: 'full',
  _disabled: {
    cursor: 'not-allowed',
    opacity: '[0.45]',
  },
  _focusVisible: focusRing,
});

export const controlRow = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
  justifyContent: 'space-between',
});

export const controlCluster = css({
  alignItems: 'center',
  display: 'flex',
  gap: '2',
  minWidth: '[0]',
});

export const controlButton = cva({
  base: {
    alignItems: 'center',
    backgroundColor: 'surfaceContainerHigh/80',
    border: '0',
    borderRadius: 'full',
    color: 'onSurface',
    cursor: 'pointer',
    display: 'inline-flex',
    flexShrink: '[0]',
    height: '11',
    justifyContent: 'center',
    transitionDuration: '200',
    transitionProperty: '[background-color, color, opacity]',
    width: '11',
    _disabled: {
      cursor: 'not-allowed',
      opacity: '[0.45]',
    },
    _focusVisible: focusRing,
    _hover: {
      backgroundColor: 'surfaceContainerHighest',
    },
  },
  variants: {
    primary: {
      true: {
        backgroundColor: 'primary',
        color: 'onPrimary',
        _hover: {
          backgroundColor: 'primary/90',
        },
      },
    },
  },
});

export const controlIcon = css({
  height: '5',
  width: '5',
});

export const exitButton = css({
  position: 'absolute',
  right: '5',
  top: '5',
  zIndex: '[5]',
  sm: {
    right: '6',
    top: '6',
  },
});

export const volume = css({
  display: 'none',
  width: '[min(12rem, 18vw)]',
  sm: {
    display: 'block',
  },
});

export const compactVolume = css({
  color: 'onSurfaceVariant',
  fontFamily: 'mono',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
  minWidth: '[2.5rem]',
  textAlign: 'right',
});
