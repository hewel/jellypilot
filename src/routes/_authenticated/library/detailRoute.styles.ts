import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]';

export const stack = css({
  display: 'grid',
  gap: '6',
  maxWidth: '[100%]',
  minWidth: '[0]',
});

export const content = css({
  display: 'grid',
  gap: '6',
  marginInline: 'auto',
  maxWidth: '[min(1400px, 100%)]',
  minWidth: '[0]',
  px: '4',
  py: '6',
  width: 'full',
  sm: {
    px: '6',
  },
  lg: {
    px: '10',
  },
  xl: {
    px: '12',
  },
});
export const overview = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '22',
  maxWidth: '[1100px]',
  lg: {
    fontSize: '15',
    lineHeight: '24',
  },
});

export const pillRow = css({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const genre = css({
  borderColor: 'outlineVariant',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant/90',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  px: '3',
  py: '1',
  textTransform: 'uppercase',
});

export const pillButton = css({
  borderRadius: 'full',
  boxSizing: 'border-box',
  maxWidth: '[100%]',
});

export const playIcon = css({
  fill: '[currentColor]',
  height: '4',
  width: '4',
});

export const icon4 = css({
  height: '4',
  width: '4',
});

export const icon6 = css({
  height: '6',
  width: '6',
});

export const spinner = css({
  animation: '[spin 1s linear infinite]',
});

export const error = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  px: '6',
});

export const skeletonHero = css({
  animation: pulse,
  bg: 'surfaceContainerLowest/60',
  height: '[clamp(280px, 44vh, 560px)]',
});

export const skeletonContent = css({
  display: 'grid',
  gap: '4',
  marginInline: 'auto',
  maxWidth: '[1400px]',
  pb: '2',
  px: '6',
  pt: '2',
  width: 'full',
  lg: {
    px: '10',
  },
  xl: {
    px: '12',
  },
});

export const skeletonLine = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/60',
  borderRadius: 'md',
  height: '4',
  maxWidth: '[1100px]',
  width: 'full',
});

export const skeletonLineShort = css({
  maxWidth: '[900px]',
  width: '[83.333%]',
});

export const skeletonPill = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'full',
  height: '7',
  width: '24',
});

export const section = css({
  display: 'grid',
  gap: '4',
});

export const sectionCompact = css({
  display: 'grid',
  gap: '3',
});

export const sectionHeader = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  sm: {
    alignItems: 'flex-end',
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
});

export const sectionTitle = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const titleSmall = css({
  color: 'onSurface',
  fontSize: '16',
  fontWeight: 'semibold',
  lineHeight: '24',
});

export const sectionSubtitle = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
});

export const fadeList = css({
  animation: '[fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards]',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
});

export const seasonTabs = css({
  bg: 'surfaceContainerLow/70',
  borderColor: 'outlineVariant',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'flex',
  gap: '2',
  overflowX: 'auto',
  p: '2',
});

export const seasonItem = css({
  flexShrink: '[0]',
});

export const selectedSeason = css({
  bg: 'secondaryContainer/45',
  borderColor: 'secondary',
  color: 'onSecondaryContainer',
});

export const selectWrap = css({
  maxWidth: '[20rem]',
});

export const episodeCard = css({
  alignItems: 'center',
  display: 'grid',
  gap: '4',
  gridTemplateColumns: '[1fr]',
  padding: '[{spacing.3} !important]',
  sm: {
    gridTemplateColumns: '[160px minmax(0, 1fr) auto]',
  },
  lg: {
    gridTemplateColumns: '[220px minmax(0, 1fr) auto]',
  },
});

export const episodeImageWrap = css({
  aspectRatio: '[16 / 9]',
  bg: 'surfaceContainerLowest/60',
  borderRadius: 'lg',
  display: 'none',
  overflow: 'hidden',
  width: '[160px]',
  sm: {
    display: 'block',
  },
  lg: {
    width: '[220px]',
  },
});

export const episodeFallback = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  fontSize: '11',
  fontWeight: 'bold',
  height: 'full',
  justifyContent: 'center',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const image = css({
  height: 'full',
  objectFit: 'cover',
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  outlineOffset: '[-1px]',
  width: 'full',
});

export const episodeCopy = css({
  display: 'grid',
  gap: '1_5',
  minWidth: '[0]',
});

export const episodeMeta = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const episodeLabel = css({
  color: 'secondary',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const muted = css({
  color: 'onSurfaceVariant/70',
  fontSize: '12',
  lineHeight: '16',
});

export const progressText = css({
  color: 'secondary',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
  lineHeight: '16',
});

export const episodeLink = css({
  color: 'onSurface',
  display: 'block',
  fontSize: '16',
  fontWeight: 'semibold',
  lineHeight: '24',
  overflow: 'hidden',
  textDecoration: 'none',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  _hover: {
    textDecoration: 'underline',
  },
});

export const actionCell = css({
  display: 'flex',
  flexShrink: '[0]',
});

export const episodeButton = css({
  borderRadius: 'full',
  fontSize: '14',
  fontWeight: 'semibold',
  letterSpacing: '[0]',
  lineHeight: '20',
  px: '5',
  py: '2',
  textTransform: 'uppercase',
});

export const skeletonTitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'md',
  height: '6',
  width: '[11rem]',
});

export const skeletonEpisodeImage = css({
  animation: pulse,
  aspectRatio: '[16 / 9]',
  bg: 'surfaceContainerLowest/60',
  borderRadius: 'lg',
  display: 'none',
  overflow: 'hidden',
  width: '[160px]',
  sm: {
    display: 'block',
  },
  lg: {
    width: '[220px]',
  },
});

export const skeletonButton = css({
  animation: pulse,
  bg: 'primaryContainer/40',
  borderRadius: 'full',
  height: '10',
  width: '24',
});

export const skeletonMiniLine = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'md',
  height: '3',
  width: '14',
});

export const skeletonSmallPill = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/60',
  borderRadius: 'full',
  height: '6',
  width: '20',
});

export const skeletonEpisodeTitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/80',
  borderRadius: 'md',
  height: '5',
  width: '[80%]',
});
