import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

const titleClamp = {
  display: '[-webkit-box]',
  WebkitBoxOrient: 'vertical',
  WebkitLineClamp: '[1]',
} satisfies Record<string, string>;

export const homeCard = css({
  color: '[inherit]',
  display: 'block',
  minWidth: '[0]',
  width: 'full',
});

/** Artwork-only control so titles/hover cards stay outside playback/open actions. */
export const homeCardAction = css({
  appearance: 'none',
  bg: '[transparent]',
  border: 'none',
  borderRadius: '2xl',
  color: '[inherit]',
  cursor: 'pointer',
  display: 'block',
  fontFamily: 'sans',
  p: '0',
  textAlign: 'left',
  textDecoration: 'none',
  transform: '[scale3d(1, 1, 1)]',
  transitionDuration: '200',
  transitionProperty: '[transform]',
  transitionTimingFunction: 'standard',
  width: 'full',
  '& img': {
    transform: '[scale3d(1, 1, 1)]',
  },
  _hover: {
    boxShadow: 'xl',
    '& [data-play-badge]': {
      bg: 'surface/85',
      boxShadow: 'xl',
      transform: '[translate3d(-50%, -50%, 0) scale3d(1.02, 1.02, 1)]',
    },
    '& img': {
      transform: '[scale3d(1.06, 1.06, 1)]',
    },
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
    boxShadow: 'xl',
    '& [data-play-badge]': {
      bg: 'surface/85',
      boxShadow: 'xl',
      transform: '[translate3d(-50%, -50%, 0) scale3d(1.02, 1.02, 1)]',
    },
    '& img': {
      transform: '[scale3d(1.06, 1.06, 1)]',
    },
  },
  _active: {
    transform: '[scale3d(0.96, 0.96, 1)]',
  },
  _disabled: {
    cursor: 'wait',
    '& [data-play-badge]': {
      opacity: '[0.55]',
    },
  },
  '@media (prefers-reduced-motion: reduce)': {
    _hover: {
      '& [data-play-badge]': {
        transform: '[translate3d(-50%, -50%, 0)]',
      },
      '& img': {
        transform: '[none]',
      },
    },
    _focusVisible: {
      '& [data-play-badge]': {
        transform: '[translate3d(-50%, -50%, 0)]',
      },
      '& img': {
        transform: '[none]',
      },
    },
  },
});

export const artwork = css({
  bg: 'surfaceContainerLowest',
  overflow: 'hidden',
  position: 'relative',
});

export const homeArtwork = css({
  borderRadius: '2xl',
  boxShadow: 'md',
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  outlineOffset: '[-1px]',
});

export const aspect = {
  poster: css({ aspectRatio: '[2 / 3]' }),
  video: css({ aspectRatio: '[16 / 9]' }),
};

export const fallback = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexDirection: 'column',
  fontSize: '11',
  fontWeight: 'bold',
  gap: '2',
  height: 'full',
  justifyContent: 'center',
  letterSpacing: '8',
  lineHeight: '16',
  px: '4',
  textAlign: 'center',
  textTransform: 'uppercase',
});

export const directPlaybackFallback = css({
  height: 'full',
  width: 'full',
});

export const fallbackIcon = css({
  height: '5',
  width: '5',
});

export const playBadge = css({
  alignItems: 'center',
  backdropFilter: '[blur(8px)]',
  bg: 'surface/70',
  borderRadius: 'full',
  boxShadow: 'lg',
  color: 'onSurface',
  display: 'inline-flex',
  height: '12',
  justifyContent: 'center',
  left: '[50%]',
  pointerEvents: 'none',
  position: 'absolute',
  top: '[50%]',
  transform: '[translate3d(-50%, -50%, 0)]',
  transitionDuration: '200',
  transitionProperty: '[background-color, box-shadow, transform]',
  transitionTimingFunction: 'standard',
  width: '12',
  zIndex: '10',
  '@media (prefers-reduced-motion: reduce)': {
    transitionProperty: '[background-color, box-shadow]',
  },
});

export const playIcon = css({
  fill: '[currentColor]',
  height: '5',
  // Optical centering for the asymmetric play triangle.
  ml: '0_5',
  width: '5',
});

export const image = css({
  height: 'full',
  objectFit: 'cover',
  transitionDuration: '200',
  transitionProperty: '[transform]',
  width: 'full',
  '@media (prefers-reduced-motion: reduce)': {
    transitionProperty: '[none]',
  },
});

export const homeImage = css({
  outline: 'none',
});

export const homeProgressTrack = css({
  bg: 'surfaceContainerHighest/75',
  bottom: '0',
  height: '1',
  left: '0',
  overflow: 'hidden',
  position: 'absolute',
  right: '0',
  zIndex: '10',
});

export const homeProgressBar = css({
  bg: 'secondary',
  height: 'full',
});

export const homeBusyOverlay = css({
  alignItems: 'center',
  backdropFilter: '[blur(4px)]',
  bg: 'surface/70',
  color: 'onSurface',
  display: 'flex',
  fontSize: '14',
  fontWeight: 'semibold',
  gap: '2',
  inset: '[0]',
  justifyContent: 'center',
  lineHeight: '20',
  position: 'absolute',
  zIndex: '20',
});

export const homeBusyIcon = css({
  animation: '[spin 1s {easings.linear} infinite]',
  height: '4',
  width: '4',
});

export const favoriteBadge = css({
  alignItems: 'center',
  bg: 'secondary',
  borderRadius: 'full',
  boxShadow: 'lg',
  color: 'onSecondary',
  display: 'inline-flex',
  height: '7',
  justifyContent: 'center',
  position: 'absolute',
  right: '2',
  top: '2',
  width: '7',
});

export const favoriteIcon = css({
  height: '4',
  width: '4',
});

export const belowMeta = css({
  alignItems: 'center',
  display: 'flex',
  gap: '2',
  minWidth: '[0]',
  pt: '3',
});

export const belowCopy = css({
  display: 'grid',
  flexGrow: '[1]',
  gap: '0_5',
  minWidth: '[0]',
});

export const title = css({
  color: 'onSurface',
  ...titleClamp,
  fontSize: '16',
  fontWeight: 'semibold',
  lineHeight: '24',
  overflow: 'hidden',
});

/** Title link to the item detail page; wraps the hover-card trigger. */
export const homeTitleLink = css({
  borderRadius: 'md',
  color: '[inherit]',
  display: 'block',
  minWidth: '[0]',
  textDecoration: 'none',
  width: 'full',
  _hover: {
    textDecoration: 'underline',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
});

export const titleHoverTrigger = css({
  display: 'block',
  minWidth: '[0]',
  width: 'full',
  cursor: 'pointer',
});

export const homeSubtitle = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '20',
});

export const playedBadge = css({
  alignItems: 'center',
  color: 'tertiary',
  display: 'inline-flex',
  flexShrink: '[0]',
  height: '5',
  justifyContent: 'center',
  width: '5',
});

export const playedIcon = css({
  height: '4',
  width: '4',
});

export const skeleton = css({
  animation: pulse,
});

export const skeletonBody = css({
  display: 'grid',
  gap: '2',
  pb: '3',
  pt: '2',
  px: '4',
});

export const skeletonTitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/80',
  borderRadius: 'md',
  height: '4',
  width: '[80%]',
});

export const skeletonSubtitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/60',
  borderRadius: 'md',
  height: '3',
  width: '[60%]',
});
