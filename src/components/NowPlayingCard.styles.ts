import { css } from '@styled-system/css';

export const root = css({
  display: 'grid',
  gap: '6',
  overflow: 'hidden',
  position: 'relative',
  '&:hover > [data-part="hover-glow"]': {
    opacity: '[1]',
  },
});

export const bareRoot = css({
  display: 'grid',
  gap: '5',
});

export const hoverGlow = css({
  backgroundImage: '[linear-gradient(90deg, {colors.primary/5}, {colors.secondary/5})]',
  inset: '0',
  opacity: '[0]',
  pointerEvents: 'none',
  position: 'absolute',
  transitionDuration: '500',
  transitionProperty: '[opacity]',
});

export const header = css({
  position: 'relative',
  zIndex: '10',
});

export const headerCopy = css({
  display: 'grid',
  gap: '1',
});

export const headerBare = css({
  paddingRight: '[9rem]',
});

export const headerFramed = css({
  maxWidth: '[70%]',
});

export const eyebrow = css({
  color: 'secondary',
  fontSize: '11',
  lineHeight: '16',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  textTransform: 'uppercase',
});

export const titleRow = css({
  display: 'flex',
  alignItems: 'center',
  gap: '3',
});

export const title = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontWeight: 'bold',
  letterSpacing: '[0]',
  overflow: 'hidden',
  paddingRight: '2',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const titleBare = css({
  fontSize: '20',
  lineHeight: '28',
});

export const titleFramed = css({
  fontSize: '24',
  lineHeight: '32',
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
  p: '4',
  boxShadow: 'inner',
  zIndex: '10',
  backdropFilter: '[blur(4px)]',
  bg: 'surfaceContainerLowest/50',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
} as const;

export const panel = css(panelBase);

export const timeRow = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  mb: '2_5',
  color: 'onSurfaceVariant',
  fontFamily: 'mono',
  fontSize: '11',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
});

export const emptyTrack = css({
  bg: 'surfaceContainerHigh/60',
  borderRadius: 'full',
  height: '2',
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
  backgroundImage: '[linear-gradient(90deg, {colors.primary}, {colors.primaryGradientEnd})]',
  boxShadow: '[0 0 10px {colors.primary/35}]',
});

export const secondaryRange = css({
  backgroundImage: '[linear-gradient(90deg, {colors.secondary}, {colors.primary})]',
  boxShadow: '[0 0 8px {colors.secondary/40}]',
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
    boxShadow: '[0 0 12px {colors.onSurface/40}]',
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
  gap: '3',
  zIndex: '10',
});

export const controlsBare = css({
  justifyContent: 'center',
});

export const controlsFramed = css({
  flexWrap: 'wrap',
  gap: '4',
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
  minWidth: '[8rem]',
  overflow: 'hidden',
  position: 'relative',
});

export const iconSlot = css({
  display: 'inline-grid',
  height: '5',
  placeItems: 'center',
  position: 'relative',
  width: '5',
});

export const contextualIcon = css({
  height: '5',
  position: 'absolute',
  transitionDuration: '200',
  transitionProperty: '[filter, opacity, transform]',
  transitionTimingFunction: 'standard',
  width: '5',
});

export const iconVisible = css({
  filter: '[blur(0)]',
  opacity: '[1]',
  transform: '[scale(1)]',
});

export const iconHidden = css({
  filter: '[blur(4px)]',
  opacity: '[0]',
  transform: '[scale(0.25)]',
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
  letterSpacing: '[0]',
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

export const pillButton = css({
  borderRadius: 'full',
});

export const playIcon = css({
  fill: '[currentColor]',
  height: '[1.125rem]',
  width: '[1.125rem]',
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
  filter: '[drop-shadow(0 0 6px {colors.secondary/15})]',
  fontFamily: 'mono',
  fontSize: '13',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
  textAlign: 'right',
  width: '12',
});

export const card = css({
  display: 'grid',
  gap: '6',
  overflow: 'hidden',
  position: 'relative',
});
