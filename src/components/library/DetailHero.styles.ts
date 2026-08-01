import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

const lineClamp = (lines: string) =>
  ({
    display: '[-webkit-box]',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: `[${lines}]`,
  }) satisfies Record<string, string>;

/* Compact cinematic hero: a full-bleed backdrop carries identity while a
 * separate portrait poster and an elevated glass copy panel hold the metadata.
 * The shell drops its gutter on detail pages, so the backdrop runs edge to edge
 * and the content column owns the responsive gutter. */
export const hero = css({
  display: 'flex',
  flexDirection: 'column',
  justifyContent: 'flex-end',
  minHeight: '[clamp(24rem, 50vh, 30rem)]',
  overflow: 'hidden',
  position: 'relative',
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
    '[linear-gradient(to top, {colors.background} 4%, {colors.background/72} 38%, transparent 74%), linear-gradient(to right, {colors.background/82} 0%, transparent 58%), linear-gradient(to bottom, {colors.background/62} 0%, transparent 26%)]',
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
  minHeight: '10',
  minWidth: '10',
  px: '3',
  py: '1_5',
  position: 'absolute',
  top: '4',
  transitionDuration: '150',
  transitionProperty: '[color, background-color]',
  zIndex: '20',
  _hover: {
    color: 'onSurface',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
  lg: {
    left: '8',
  },
});

/* Content column: poster + glass copy panel sit side by side on desktop and
 * collapse to a single full-width panel below 800px. */
export const content = css({
  alignItems: 'flex-end',
  display: 'flex',
  gap: '6',
  minWidth: '[0]',
  pb: '6',
  position: 'relative',
  pt: '12',
  px: '4',
  width: 'full',
  zIndex: '10',
  lg: {
    px: '8',
  },
});

export const poster = css({
  aspectRatio: '[2 / 3]',
  borderRadius: '2xl',
  boxShadow: '2xl',
  flex: 'none',
  objectFit: 'cover',
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  overflow: 'hidden',
  width: '[clamp(10rem, 15vw, 13rem)]',
  _belowPoster: {
    display: 'none',
  },
});

export const posterFallback = css({
  alignItems: 'center',
  bg: 'surfaceContainerHigh',
  color: 'onSurface/20',
  display: 'flex',
  fontFamily: 'display',
  fontSize: '[3rem]',
  fontWeight: 'extrabold',
  height: 'full',
  justifyContent: 'center',
  width: 'full',
});

export const glassPanel = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surface/72',
  borderColor: 'onSurface/10',
  borderRadius: '3xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  display: 'grid',
  gap: '4',
  maxWidth: '[46rem]',
  minWidth: '[0]',
  p: '6',
  width: 'full',
});

export const metaRow = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const chip = css({
  alignItems: 'center',
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

export const ratingChip = css({
  color: 'warning',
  fontVariantNumeric: 'tabular-nums',
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
  fontSize: '[36px]',
  fontWeight: 'extrabold',
  letterSpacing: '[-0.02em]',
  lineHeight: '[1.08]',
  m: '0',
  overflowWrap: 'break-word',
  sm: {
    fontSize: '[44px]',
  },
  lg: {
    fontSize: '[52px]',
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
  m: '0',
});

export const metaLink = css({
  color: 'secondary',
  textDecoration: 'none',
  _hover: {
    textDecoration: 'underline',
  },
});

export const overviewWrap = css({
  display: 'grid',
  gap: '1',
});

export const overview = css({
  color: 'onSurfaceVariant',
  fontSize: '15',
  lineHeight: 'relaxed',
  m: '0',
  overflow: 'hidden',
});

export const overviewClamped = css({
  ...lineClamp('2'),
});

export const overviewToggle = css({
  appearance: 'none',
  bg: '[transparent]',
  border: 'none',
  color: 'secondary',
  cursor: 'pointer',
  fontSize: '13',
  fontWeight: 'semibold',
  justifySelf: 'start',
  minHeight: '10',
  minWidth: '10',
  m: '0',
  p: '0',
  pl: '0',
  _hover: {
    textDecoration: 'underline',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
});

export const progressWrap = css({
  display: 'grid',
  gap: '1_5',
});

export const progressBar = css({
  bg: 'onSurface/15',
  borderRadius: 'full',
  height: '1_5',
  overflow: 'hidden',
  width: 'full',
});

export const progressFill = css({
  bg: 'secondary',
  borderRadius: 'full',
  height: 'full',
});

export const progressLabel = css({
  color: 'onSurfaceVariant',
  fontSize: '13',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
  lineHeight: '20',
});

export const actions = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
});

export const icon4 = css({
  height: '4',
  width: '4',
});

/* Summary surface below the hero: Genres / Creators / Cast columns plus a
 * fixed-height deferred technical row. */
export const summary = css({
  display: 'grid',
  gap: '6',
  minWidth: '[0]',
  px: '4',
  width: 'full',
  lg: {
    px: '8',
  },
});

export const summaryColumns = css({
  display: 'grid',
  gap: '6',
  minWidth: '[0]',
  _posterAndUp: {
    gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
  },
  lg: {
    gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
  },
});

export const summaryColumn = css({
  display: 'grid',
  gap: '2',
  minWidth: '[0]',
});

export const summaryLabel = css({
  color: 'secondary',
  fontSize: '12',
  fontWeight: 'semibold',
  letterSpacing: '18',
  lineHeight: '16',
  m: '0',
  textTransform: 'uppercase',
});

export const summaryValues = css({
  color: 'onSurface',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '14',
  gap: '1_5',
  lineHeight: '22',
  m: '0',
});

export const summaryValue = css({
  color: 'onSurface',
});

export const summarySeparator = css({
  color: 'outline/40',
});

export const summaryMore = css({
  color: 'onSurfaceVariant',
  fontVariantNumeric: 'tabular-nums',
});

export const technicalRows = css({
  borderTopColor: 'outlineVariant/50',
  borderTopStyle: 'solid',
  borderTopWidth: '1px',
  display: 'grid',
  gap: '2',
  minHeight: '12',
  pt: '4',
});

export const technicalRow = css({
  alignItems: 'baseline',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const technicalLabel = css({
  color: 'secondary',
  fontSize: '12',
  fontWeight: 'semibold',
  letterSpacing: '18',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const technicalValue = css({
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '22',
});

export const technicalLoading = css({
  color: 'onSurfaceVariant/70',
  fontSize: '13',
  lineHeight: '20',
});

/* Skeleton reserves the poster, glass panel, summary, and technical geometry
 * so late core/stream data does not shift the page. */
export const skeletonHero = css({
  bg: 'surfaceContainerLow',
  display: 'flex',
  flexDirection: 'column',
  justifyContent: 'flex-end',
  minHeight: '[clamp(24rem, 50vh, 30rem)]',
  overflow: 'hidden',
  position: 'relative',
});

export const skeletonBackdrop = css({
  animation: pulse,
  bg: 'surfaceContainer/55',
  inset: '[0]',
  position: 'absolute',
});

export const skeletonContent = css({
  alignItems: 'flex-end',
  display: 'flex',
  gap: '6',
  minWidth: '[0]',
  pb: '8',
  position: 'relative',
  pt: '20',
  px: '4',
  width: 'full',
  zIndex: '[1]',
  lg: {
    px: '8',
  },
});

export const skeletonPoster = css({
  animation: pulse,
  aspectRatio: '[2 / 3]',
  bg: 'onSurface/10',
  borderRadius: '2xl',
  flex: 'none',
  width: '[clamp(10rem, 15vw, 13rem)]',
  _belowPoster: {
    display: 'none',
  },
});

export const skeletonPanel = css({
  display: 'grid',
  gap: '4',
  maxWidth: '[46rem]',
  minWidth: '[0]',
  width: 'full',
});

export const skeletonBadge = css({
  animation: pulse,
  bg: 'onSurface/10',
  borderRadius: 'full',
  height: '7',
  width: '24',
});

export const skeletonTitle = css({
  animation: pulse,
  bg: 'onSurface/15',
  borderRadius: 'lg',
  height: '14',
  maxWidth: '[30rem]',
  width: '[70%]',
});

export const skeletonLine = css({
  animation: pulse,
  bg: 'onSurface/10',
  borderRadius: 'md',
  height: '5',
  maxWidth: '[26rem]',
  width: '[52%]',
});

export const skeletonActions = css({
  animation: pulse,
  bg: 'onSurface/10',
  borderRadius: 'full',
  height: '11',
  width: '[12rem]',
});

export const skeletonSummary = css({
  display: 'grid',
  gap: '6',
  minWidth: '[0]',
  px: '4',
  width: 'full',
  lg: {
    gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
    px: '8',
  },
});

export const skeletonSummaryColumn = css({
  display: 'grid',
  gap: '2',
});

export const skeletonSummaryLabel = css({
  animation: pulse,
  bg: 'onSurface/10',
  borderRadius: 'sm',
  height: '4',
  width: '16',
});

export const skeletonSummaryLine = css({
  animation: pulse,
  bg: 'onSurface/10',
  borderRadius: 'md',
  height: '4',
  width: '[80%]',
});
