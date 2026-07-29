import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

/* Cinematic detail hero: the shell drops its gutter on detail pages, so the
 * backdrop runs edge to edge. Layered scrims keep badges → display title →
 * genres → actions → overview + glass info panel legible over the art. */
export const hero = css({
  display: 'flex',
  flexDirection: 'column',
  justifyContent: 'flex-end',
  minHeight: '[540px]',
  overflow: 'hidden',
  position: 'relative',
  lg: {
    minHeight: '[660px]',
  },
});

export const backdrop = css({
  bg: 'surfaceContainerLow',
  inset: '[0]',
  position: 'absolute',
});

export const backdropImage = css({
  height: 'full',
  objectFit: 'cover',
  width: 'full',
});

export const backdropFallback = css({
  alignItems: 'center',
  color: 'onSurface/10',
  display: 'flex',
  fontFamily: 'display',
  fontSize: '[14rem]',
  fontWeight: 'extrabold',
  inset: '[0]',
  justifyContent: 'center',
  lineHeight: '[1]',
  position: 'absolute',
  userSelect: 'none',
});

export const scrim = css({
  backgroundImage:
    '[linear-gradient(to top, {colors.background} 2%, {colors.background/70} 34%, transparent 72%), linear-gradient(to right, {colors.background/80} 0%, transparent 55%), linear-gradient(to bottom, {colors.background/60} 0%, transparent 26%)]',
  inset: '[0]',
  position: 'absolute',
});

export const backLink = css({
  alignItems: 'center',
  appearance: 'none',
  backdropFilter: '[blur(20px)]',
  bg: 'surfaceContainer/55',
  borderColor: 'onSurface/10',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'inline-flex',
  fontSize: '14',
  gap: '1',
  left: '4',
  lineHeight: '20',
  px: '3',
  py: '1_5',
  position: 'absolute',
  top: '4',
  zIndex: '20',
  _hover: {
    color: 'onSurface',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
});

export const content = css({
  display: 'grid',
  gap: '5',
  pb: '8',
  position: 'relative',
  pt: '24',
  px: '4',
  width: 'full',
  zIndex: '10',
});

export const badgeRow = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const chip = css({
  alignItems: 'center',
  backdropFilter: '[blur(12px)]',
  bg: 'onSurface/10',
  borderColor: 'onSurface/15',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurface',
  display: 'inline-flex',
  fontSize: '12',
  fontWeight: 'semibold',
  gap: '1_5',
  lineHeight: '16',
  px: '3',
  py: '1',
});

export const metaText = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
  letterSpacing: '5',
  lineHeight: '20',
  px: '1',
});

export const title = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '[40px]',
  fontWeight: 'extrabold',
  letterSpacing: '[-0.02em]',
  lineHeight: '[1.1]',
  overflowWrap: 'break-word',
  sm: {
    fontSize: '[56px]',
  },
  lg: {
    fontSize: '[64px]',
  },
});

export const metaLine = css({
  alignItems: 'baseline',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '15',
  gap: '1_5',
  lineHeight: '22',
});

export const metaLink = css({
  color: 'secondary',
  textDecoration: 'none',
  _hover: {
    textDecoration: 'underline',
  },
});

export const genres = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '16',
  gap: '3',
  lineHeight: '24',
});

export const genreSeparator = css({
  color: 'outline/40',
});

export const actions = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
  mt: '2',
});

export const infoGrid = css({
  display: 'grid',
  gap: '6',
  mt: '4',
  lg: {
    gridTemplateColumns: '[minmax(0, 2fr) minmax(0, 1fr)]',
  },
});

export const overview = css({
  alignSelf: 'start',
  color: 'onSurfaceVariant',
  fontSize: '16',
  lineHeight: 'relaxed',
  maxWidth: '[64ch]',
  lg: {
    fontSize: '18',
  },
});

export const infoPanel = css({
  alignContent: 'start',
  alignSelf: 'start',
  backdropFilter: '[blur(20px)]',
  bg: 'surfaceContainer/55',
  borderColor: 'onSurface/10',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'grid',
  gap: '5',
  m: '0',
  p: '6',
  width: 'full',
});

export const infoItem = css({
  display: 'grid',
  gap: '1',
});

export const infoLabel = css({
  color: 'secondary',
  fontSize: '12',
  fontWeight: 'semibold',
  letterSpacing: '18',
  lineHeight: '16',
  m: '0',
  textTransform: 'uppercase',
});

export const infoValue = css({
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '22',
  m: '0',
});

export const icon4 = css({
  height: '4',
  width: '4',
});

export const skeletonHero = css({
  animation: pulse,
  bg: 'surfaceContainerLow/60',
  minHeight: '[540px]',
  lg: {
    minHeight: '[660px]',
  },
});
