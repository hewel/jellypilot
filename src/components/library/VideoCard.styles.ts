import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]';

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

export const artwork = css({
  bg: 'surfaceContainerLowest',
  overflow: 'hidden',
  position: 'relative',
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
  letterSpacing: '[0.08em]',
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
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  outlineOffset: '[-1px]',
  width: 'full',
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

export const subtitle = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
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
