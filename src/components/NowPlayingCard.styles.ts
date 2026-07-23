import { css, cva } from '@styled-system/css';

/**
 * Narrow contracts for Now Playing transport/timeline rows.
 * Base: wrap controls and allow play button to shrink; sm+ keeps denser desktop row.
 */
export const nowPlayingCardNarrowLayout = {
  rootMaxWidth: '[100%]',
  rootMinWidth: '[0]',
  controlsBaseWrap: 'wrap',
  controlsBaseGap: '2',
  playMinWidthBase: '[0]',
  playMinWidthSm: '[8rem]',
  timeLabelMinWidth: '[0]',
} as const;

export const root = css({
  display: 'grid',
  gap: '5',
  maxWidth: nowPlayingCardNarrowLayout.rootMaxWidth,
  minWidth: nowPlayingCardNarrowLayout.rootMinWidth,
  width: 'full',
});

export const header = css({
  maxWidth: '[100%]',
  minWidth: '[0]',
  position: 'relative',
  width: 'full',
  zIndex: '10',
});

export const headerCopy = css({
  display: 'grid',
  gap: '1',
  maxWidth: '[100%]',
  minWidth: '[0]',
  paddingRight: '[9rem]',
});

export const titleRow = css({
  display: 'flex',
  alignItems: 'center',
  gap: '3',
});

export const title = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '20',
  lineHeight: '28',
  fontWeight: 'bold',
  letterSpacing: '0',
  overflow: 'hidden',
  paddingRight: '2',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const equalizer = css({
  display: 'flex',
  height: '6',
  width: '8',
  flexShrink: '[0]',
  alignItems: 'flex-end',
  gap: '1_5',
  pb: '1',
  userSelect: 'none',
});

export const waveBar = css({
  animationDirection: 'alternate',
  animationIterationCount: 'infinite',
  animationName: '[wave-bounce]',
  animationTimingFunction: '[ease-in-out]',
  borderRadius: 'full',
  height: 'full',
  transformOrigin: 'bottom',
  width: '1_5',
  willChange: 'transform',
});

export const wavePrimary = css({
  bg: 'primary',
});

export const waveSecondary = css({
  bg: 'secondary',
});

export const waveTiming = {
  a: css({ animationDuration: '[0.8s]' }),
  b: css({ animationDelay: '[0.15s]', animationDuration: '[0.6s]' }),
  c: css({ animationDelay: '[0.3s]', animationDuration: '[0.9s]' }),
  d: css({ animationDelay: '[0.45s]', animationDuration: '[0.7s]' }),
} as const;

export const subtitle = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'medium',
});

export const badgePlacement = css({
  position: 'absolute',
  right: '0',
  top: '0',
});

const panelBase = {
  position: 'relative',
  borderRadius: '3xl',
  boxSizing: 'border-box',
  maxWidth: '[100%]',
  minWidth: '[0]',
  p: '4',
  boxShadow: 'inner',
  width: 'full',
  zIndex: '10',
  bg: 'surfaceContainerLowest/50',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
} as const;

export const panel = css(panelBase);

export const timeRow = css({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  justifyContent: 'space-between',
  mb: '2_5',
  color: 'onSurfaceVariant',
  fontFamily: 'mono',
  fontSize: '11',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
  '& > *': {
    minWidth: nowPlayingCardNarrowLayout.timeLabelMinWidth,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
});

export const statePanel = css({
  ...panelBase,
  display: 'grid',
  gap: '2',
  justifyItems: 'center',
  py: '8',
  textAlign: 'center',
});

export const stateIcon = css({
  color: 'onSurfaceVariant',
  display: 'inline-flex',
  '& svg': {
    height: '8',
    width: '8',
  },
});

export const stateTitle = css({
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const stateMessage = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  maxWidth: '[24rem]',
});

export const sliderRoot = css({
  display: 'flex',
  width: 'full',
  flexDirection: 'column',
  gap: '2_5',
  '&:disabled, &[data-disabled]': {
    opacity: '[0.5]',
  },
});

export const sliderControl = css({
  position: 'relative',
  display: 'flex',
  height: '10',
  alignItems: 'center',
  cursor: 'pointer',
});

export const sliderTrack = css({
  flexGrow: '[1]',
  overflow: 'hidden',
  borderRadius: 'full',
  bg: 'surfaceContainerHighest/80',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/30',
  height: '2_5',
});

export const sliderRange = css({
  borderRadius: 'full',
  height: 'full',
  transitionDuration: '150',
  transitionProperty: '[width, transform]',
});

export const primaryRange = css({
  bg: 'primary',
});

export const secondaryRange = css({
  bg: 'secondary',
});

export const thumb = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: 'full',
  bg: 'onSurface',
  borderWidth: '2px',
  borderStyle: 'solid',
  borderColor: 'surfaceContainerLowest',
  boxShadow: '[0 10px 15px -3px {colors.surfaceContainerLowest/50}]',
  cursor: 'grab',
  height: '[1.375rem]',
  outline: 'none',
  transitionDuration: '200',
  transitionProperty: '[box-shadow, transform]',
  width: '[1.375rem]',
  _hover: {
    transform: '[scale(1.1)]',
  },
  _active: {
    cursor: 'grabbing',
  },
  '&[data-focus-visible]': {
    boxShadow: '[0 0 0 2px {colors.primary/50}]',
  },
});

export const controls = css({
  position: 'relative',
  display: 'flex',
  alignItems: 'center',
  flexWrap: nowPlayingCardNarrowLayout.controlsBaseWrap,
  gap: nowPlayingCardNarrowLayout.controlsBaseGap,
  justifyContent: 'center',
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
  zIndex: '10',
});

export const iconButton = css({
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
  borderRadius: 'full',
  _hover: {
    bg: 'secondary/5',
    borderColor: 'secondary',
    color: 'secondary',
  },
});

export const stopButton = css({
  _hover: {
    bg: 'error/5',
    borderColor: 'error',
    color: 'error',
  },
});

export const playPauseButton = css({
  borderRadius: 'full',
  flex: '[1 1 auto]',
  maxWidth: '[100%]',
  minWidth: nowPlayingCardNarrowLayout.playMinWidthBase,
  overflow: 'hidden',
  position: 'relative',
  sm: {
    flex: 'none',
    minWidth: nowPlayingCardNarrowLayout.playMinWidthSm,
  },
});

export const iconSlot = css({
  display: 'inline-grid',
  height: '5',
  placeItems: 'center',
  position: 'relative',
  width: '5',
});

export const contextualIcon = cva({
  base: {
    height: '5',
    position: 'absolute',
    transitionDuration: '200',
    transitionProperty: '[filter, opacity, transform]',
    transitionTimingFunction: 'standard',
    width: '5',
  },
  variants: {
    visible: {
      true: {
        filter: '[blur(0)]',
        opacity: '[1]',
        transform: '[scale(1)]',
      },
      false: {
        filter: '[blur(4px)]',
        opacity: '[0]',
        transform: '[scale(0.25)]',
      },
    },
  },
});

export const iconDropShadow = css({
  filter: '[drop-shadow(0 2px 4px {colors.surfaceContainerLowest/30})]',
});

export const secondaryIcon = css({
  color: 'secondary',
});

export const errorIcon = css({
  color: 'error',
});

export const actionLabel = css({
  fontWeight: 'bold',
  letterSpacing: '0',
});

export const icon5 = css({
  height: '5',
  width: '5',
});

export const squareIcon = css({
  fill: '[currentColor]',
  height: '4',
  width: '4',
});

export const playIcon = css({
  fill: '[currentColor]',
  height: 'lg',
  width: 'lg',
});

export const selectPanel = css({
  ...panelBase,
  display: 'grid',
  gap: '3',
  sm: {
    gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
  },
});

export const volumePanel = css({
  ...panelBase,
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  gap: '3',
  sm: {
    flexDirection: 'row',
  },
});

export const muteButton = css({
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: '[transparent]',
  borderRadius: 'xl',
  flexShrink: '[0]',
  _hover: {
    bg: 'secondary/15',
    borderColor: 'secondary/20',
    color: 'secondary',
  },
});

export const volumeValue = css({
  color: 'secondary',
  fontFamily: 'mono',
  fontSize: '13',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
  textAlign: 'right',
  width: '12',
});
