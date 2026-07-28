import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

export const stack = css({
  display: 'grid',
  gap: '6',
  maxWidth: '[100%]',
  minWidth: '[0]',
});

export const page = css({
  display: 'grid',
  gap: '6',
  marginInline: 'auto',
  maxWidth: '[min(1400px, 100%)]',
  minWidth: '[0]',
  py: '6',
  width: 'full',
});

export const backLink = css({
  alignItems: 'center',
  appearance: 'none',
  bg: '[transparent]',
  border: 'none',
  borderRadius: 'md',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'inline-flex',
  fontSize: '14',
  gap: '1',
  justifySelf: 'start',
  lineHeight: '20',
  p: '1',
  _hover: {
    color: 'onSurface',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
});

export const hero = css({
  bg: 'surfaceContainerLow/60',
  borderColor: 'outlineVariant/60',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'grid',
  gridTemplateColumns: '[minmax(0, 1fr)]',
  overflow: 'hidden',
  lg: {
    gridTemplateColumns: '[minmax(0, 1.8fr) minmax(0, 1fr)]',
  },
});

export const heroInfo = css({
  alignContent: 'start',
  display: 'grid',
  gap: '4',
  minWidth: '[0]',
  p: '6',
  sm: {
    p: '8',
  },
  lg: {
    p: '10',
  },
});

export const badgeRow = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
});

export const typeBadge = css({
  alignItems: 'center',
  bg: 'secondaryContainer/45',
  borderColor: 'secondary/50',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSecondaryContainer',
  display: 'inline-flex',
  fontSize: '12',
  fontWeight: 'semibold',
  gap: '1_5',
  lineHeight: '16',
  px: '3',
  py: '1',
});

export const badge = css({
  borderColor: 'outlineVariant',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant',
  display: 'inline-flex',
  fontSize: '12',
  lineHeight: '16',
  px: '3',
  py: '1',
});

export const heroTitle = css({
  color: 'onSurface',
  fontSize: '32',
  fontWeight: 'bold',
  lineHeight: '40',
  overflowWrap: 'break-word',
  sm: {
    fontSize: '[40px]',
    lineHeight: '[48px]',
  },
});

export const heroMeta = css({
  alignItems: 'baseline',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '14',
  fontVariantNumeric: 'tabular-nums',
  gap: '1_5',
  lineHeight: '20',
});

export const heroMetaLink = css({
  color: 'secondary',
  textDecoration: 'none',
  _hover: {
    textDecoration: 'underline',
  },
});

export const heroOverview = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineClamp: '2',
  lineHeight: '22',
  maxWidth: '[64ch]',
  lg: {
    fontSize: '15',
    lineHeight: '24',
  },
});

export const accentBar = css({
  bg: '[linear-gradient(90deg, {colors.primary}, {colors.primary/0})]',
  borderRadius: 'full',
  height: '[3px]',
  width: '[240px]',
});

export const heroActions = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
  mt: '2',
});

export const heroArt = css({
  bg: 'surfaceContainer/80',
  borderColor: 'outlineVariant/60',
  borderTopStyle: 'solid',
  borderTopWidth: '1px',
  minHeight: '[220px]',
  overflow: 'hidden',
  position: 'relative',
  lg: {
    borderLeftStyle: 'solid',
    borderLeftWidth: '1px',
    borderTopWidth: '[0]',
    minHeight: '[320px]',
  },
});

export const heroArtImage = css({
  height: 'full',
  inset: '[0]',
  objectFit: 'cover',
  position: 'absolute',
  width: 'full',
});

export const heroArtFallback = css({
  alignItems: 'flex-end',
  color: 'onSurface/12',
  display: 'flex',
  fontSize: '[10rem]',
  fontWeight: 'bold',
  height: 'full',
  inset: '[0]',
  lineHeight: '[1]',
  p: '6',
  position: 'absolute',
  userSelect: 'none',
});

export const heroArtYear = css({
  bottom: '4',
  color: 'onSurfaceVariant/70',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
  position: 'absolute',
  right: '5',
});

export const heroArtProgress = css({
  bg: 'surfaceContainerLowest/70',
  bottom: '0',
  height: '[4px]',
  left: '0',
  position: 'absolute',
  right: '0',
});

export const heroArtProgressBar = css({
  bg: 'primary',
  height: 'full',
});

export const pillButton = css({
  borderRadius: 'full',
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

export const spinner = css({
  animation: '[spin 1s {easings.linear} infinite]',
});

export const error = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  px: '6',
});

export const skeletonHero = css({
  animation: pulse,
  bg: 'surfaceContainerLow/60',
  borderRadius: '2xl',
  height: '[320px]',
});

export const skeletonBar = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'full',
  height: '9',
  width: '[16rem]',
});
