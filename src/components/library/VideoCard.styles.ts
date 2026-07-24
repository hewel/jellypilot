import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

const titleClamp = {
  display: '[-webkit-box]',
  WebkitBoxOrient: 'vertical',
  WebkitLineClamp: '[1]',
} satisfies Record<string, string>;

export const card = css({
  bg: 'surface',
  borderColor: 'outlineVariant/80',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: 'xl',
  color: '[inherit]',
  display: 'block',
  overflow: 'hidden',
  p: '0',
  textDecoration: 'none',
  transitionDuration: '300',
  transitionProperty: '[background-color, border-color, box-shadow, transform]',
  _hover: {
    borderColor: 'primary/50',
  },
  _focusVisible: {
    boxShadow: '[0 0 0 2px color-mix(in srgb, {colors.secondary} 70%, transparent), {shadows.xl}]',
    outline: 'none',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
});

export const homeCard = css({
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
  transitionDuration: '200',
  transitionProperty: '[transform]',
  width: 'full',
  _hover: {
    transform: '[translateY(-2px)]',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  _disabled: {
    cursor: 'wait',
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

export const fallbackIcon = css({
  height: '5',
  width: '5',
});

export const image = css({
  height: 'full',
  objectFit: 'cover',
  width: 'full',
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

export const overlay = css({
  backgroundImage:
    '[linear-gradient(to top, {colors.surface} 0%, color-mix(in srgb, {colors.surface} 80%, transparent) 55%, transparent 100%)]',
  bottom: '0',
  display: 'grid',
  gap: '0_5',
  left: '0',
  pb: '2_5',
  position: 'absolute',
  pt: '10',
  px: '3',
  right: '0',
});

export const overlayPlayedBadge = css({
  alignItems: 'center',
  bg: 'tertiary',
  borderRadius: 'full',
  boxShadow: 'lg',
  color: 'onTertiary',
  display: 'inline-flex',
  height: '7',
  justifyContent: 'center',
  left: '2',
  position: 'absolute',
  top: '2',
  width: '7',
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

export const body = css({
  alignItems: 'center',
  borderTopColor: 'outlineVariant',
  borderTopStyle: 'solid',
  borderTopWidth: '1px',
  display: 'flex',
  gap: '2',
  pb: '3',
  pt: '2',
  px: '4',
});

export const homeBody = css({
  display: 'grid',
  gap: '0_5',
  minWidth: '[0]',
  pt: '3',
});

export const copy = css({
  display: 'grid',
  flexGrow: '[1]',
  gap: '1',
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

export const homeTitle = css({
  color: 'onSurface',
  ...titleClamp,
  fontSize: '16',
  fontWeight: 'semibold',
  lineHeight: '24',
  overflow: 'hidden',
});

export const subtitle = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
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
