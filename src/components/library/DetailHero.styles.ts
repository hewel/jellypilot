import { css } from '@styled-system/css';

const artworkTitleClamp = {
  display: '[-webkit-box]',
  WebkitBoxOrient: 'vertical',
  WebkitLineClamp: '[3]',
} satisfies Record<string, string>;

/**
 * Explicit narrow (360px stress) layout contracts for DetailHero.
 * Base: stacked full-width action column; sm+: row wrap (supported desktop parity).
 * Does not rely on overflow-x clipping.
 */
export const detailHeroNarrowLayout = {
  contentBox: 'border-box',
  contentWidth: 'full',
  contentMaxWidth: '[100%]',
  contentMinWidth: '[0]',
  contentPaddingXBase: '4',
  contentPaddingTopBase: '14',
  rootMinHeight: '[clamp(280px, 44vh, 560px)]',
  actionsBaseDirection: 'column',
  actionsSmDirection: 'row',
  actionBaseWidth: 'full',
  actionSmWidth: 'auto',
  titleOverflowWrap: 'anywhere',
} as const;

export const root = css({
  minHeight: detailHeroNarrowLayout.rootMinHeight,
  overflow: 'hidden',
  position: 'relative',
  width: 'full',
  maxWidth: '[100%]',
});

export const mediaLayer = css({
  inset: '0',
  position: 'absolute',
});

export const fallbackBackdrop = css({
  backgroundImage:
    '[linear-gradient(to bottom, color-mix(in srgb, {colors.primaryContainer} 30%, transparent), {colors.surface})]',
  height: 'full',
  width: 'full',
});

export const heroImage = css({
  filter: '[blur(20px) brightness(0.3)]',
  height: 'full',
  objectFit: 'cover',
  transform: '[scale(1.1)]',
  width: 'full',
});

export const scrim = css({
  backgroundImage:
    '[linear-gradient(to top, {colors.surface}, color-mix(in srgb, {colors.surface} 60%, transparent), transparent)]',
  inset: '0',
  position: 'absolute',
});

export const backButton = css({
  backdropFilter: '[blur(12px)]',
  bg: '[rgb(0 0 0 / 0.35)]',
  borderColor: '[rgb(255 255 255 / 0.15)]',
  borderRadius: 'full',
  boxShadow: '2xl',
  color: 'onPrimary',
  left: '4',
  position: 'absolute',
  top: '4',
  transitionProperty: '[background-color, border-color, transform]',
  zIndex: '20',
  _hover: {
    bg: '[rgb(0 0 0 / 0.5)]',
    borderColor: '[rgb(255 255 255 / 0.3)]',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  lg: {
    left: '8',
    top: '6',
  },
  xl: {
    left: '10',
  },
});

export const backIcon = css({
  height: '4',
  width: '4',
});

export const content = css({
  alignItems: 'flex-end',
  boxSizing: 'border-box',
  display: 'flex',
  gap: '4',
  maxWidth: detailHeroNarrowLayout.contentMaxWidth,
  minHeight: detailHeroNarrowLayout.rootMinHeight,
  minWidth: detailHeroNarrowLayout.contentMinWidth,
  pt: detailHeroNarrowLayout.contentPaddingTopBase,
  pb: '6',
  px: detailHeroNarrowLayout.contentPaddingXBase,
  position: 'relative',
  width: detailHeroNarrowLayout.contentWidth,
  zIndex: '10',
  sm: {
    gap: '6',
    px: '6',
  },
  lg: {
    gap: '8',
    pb: '8',
    pt: '16',
    px: '10',
  },
  xl: {
    gap: '10',
    pb: '10',
    px: '12',
  },
});

export const artwork = css({
  bg: 'surfaceContainerLowest/70',
  borderRadius: 'xl',
  boxShadow: '2xl',
  display: 'none',
  flexShrink: '[0]',
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  outlineOffset: '[-1px]',
  overflow: 'hidden',
  position: 'relative',
  sm: {
    display: 'block',
  },
});

export const artworkAspect = {
  poster: css({ aspectRatio: '[2 / 3]' }),
  landscape: css({ aspectRatio: '[16 / 9]' }),
};

export const artworkWidth = {
  poster: css({
    width: '[140px]',
    lg: { width: '[190px]' },
    '2xl': { width: '[220px]' },
  }),
  landscape: css({
    width: '[200px]',
    lg: { width: '[280px]' },
    '2xl': { width: '[340px]' },
  }),
};

export const artworkFallback = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  height: 'full',
  justifyContent: 'center',
  px: '4',
  textAlign: 'center',
});

export const artworkFallbackIcon = css({
  alignItems: 'center',
  bg: 'secondaryContainer',
  borderRadius: '2xl',
  color: 'secondary',
  display: 'flex',
  height: '12',
  justifyContent: 'center',
  width: '12',
});

export const artworkFallbackTitle = css({
  color: 'onSurface',
  ...artworkTitleClamp,
  fontSize: '13',
  fontWeight: 'semibold',
  lineHeight: '20',
  overflow: 'hidden',
});

export const artworkImage = css({
  height: 'full',
  objectFit: 'cover',
  width: 'full',
});

export const progressTrack = css({
  bg: 'surface/70',
  bottom: '0',
  height: '1',
  left: '0',
  position: 'absolute',
  right: '0',
});

export const progressBar = css({
  bg: 'secondary',
  height: 'full',
});

export const copy = css({
  display: 'grid',
  flex: '[1]',
  gap: '3',
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
  lg: {
    gap: '4',
  },
});

export const titleBlock = css({
  display: 'grid',
  gap: '1',
  maxWidth: '[100%]',
  minWidth: '[0]',
});

export const title = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '28',
  fontWeight: 'bold',
  letterSpacing: '0',
  lineHeight: '40',
  maxWidth: '[100%]',
  minWidth: '[0]',
  overflowWrap: detailHeroNarrowLayout.titleOverflowWrap,
  wordBreak: 'break-word',
  lg: {
    fontSize: '[2.625rem]',
    lineHeight: '[3.125rem]',
  },
  xl: {
    fontSize: '[3rem]',
    lineHeight: '[3.5rem]',
  },
});

export const subtitle = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  maxWidth: '[100%]',
  minWidth: '[0]',
  overflowWrap: 'anywhere',
  lg: {
    fontSize: '16',
    lineHeight: '24',
  },
});

export const badges = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
  // Badges are discrete chips; keep them whole and wrap to next line.
  '& > *': {
    flex: '[0 1 auto]',
    maxWidth: '[100%]',
    minWidth: '[0]',
  },
});

export const actions = css({
  // Base (incl. 360 stress): stack full-width controls so long labels never clip.
  alignItems: 'stretch',
  display: 'flex',
  flexDirection: detailHeroNarrowLayout.actionsBaseDirection,
  gap: '3',
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
  '& > *': {
    boxSizing: 'border-box',
    maxWidth: '[100%]',
    minWidth: '[0]',
    width: detailHeroNarrowLayout.actionBaseWidth,
  },
  // Sm+ restores side-by-side wrapping used at supported desktop sizes.
  sm: {
    alignItems: 'center',
    flexDirection: detailHeroNarrowLayout.actionsSmDirection,
    flexWrap: 'wrap',
    '& > *': {
      width: detailHeroNarrowLayout.actionSmWidth,
    },
  },
});
