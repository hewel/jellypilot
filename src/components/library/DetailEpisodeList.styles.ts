import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

const lineClamp = (lines: string) =>
  ({
    display: '[-webkit-box]',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: `[${lines}]`,
  }) satisfies Record<string, string>;

export const list = css({
  display: 'grid',
  gap: '3',
  minWidth: '[0]',
});

export const row = css({
  alignItems: 'center',
  bg: 'surfaceContainer/40',
  borderColor: 'onSurface/8',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'grid',
  gap: '4',
  gridTemplateColumns: '[minmax(0, 10rem) minmax(0, 1fr) auto]',
  minWidth: '[0]',
  p: '3',
  sm: {
    gridTemplateColumns: '[minmax(0, 12rem) minmax(0, 1fr) auto]',
  },
  _belowPoster: {
    gridTemplateColumns: '[minmax(0, 1fr) auto]',
  },
});

export const thumb = css({
  aspectRatio: '[16 / 9]',
  borderRadius: 'lg',
  flex: 'none',
  objectFit: 'cover',
  overflow: 'hidden',
  width: 'full',
  _belowPoster: {
    display: 'none',
  },
});

export const thumbImage = css({
  height: 'full',
  objectFit: 'cover',
  width: 'full',
});

export const thumbFallback = css({
  alignItems: 'center',
  bg: 'surfaceContainerHigh',
  color: 'onSurface/20',
  display: 'flex',
  fontSize: '14',
  fontWeight: 'semibold',
  height: 'full',
  justifyContent: 'center',
  width: 'full',
});

export const thumbIcon = css({
  height: '6',
  width: '6',
});

export const copy = css({
  display: 'grid',
  gap: '1',
  minWidth: '[0]',
});

export const titleRow = css({
  alignItems: 'baseline',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '2',
  minWidth: '[0]',
});

export const episodeCode = css({
  color: 'secondary',
  fontSize: '12',
  fontWeight: 'semibold',
  letterSpacing: '8',
  lineHeight: '16',
});

export const title = css({
  color: 'onSurface',
  fontSize: '15',
  fontWeight: 'semibold',
  lineHeight: '22',
  overflow: 'hidden',
  textDecoration: 'none',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  _hover: {
    textDecoration: 'underline',
  },
});

export const overview = css({
  color: 'onSurfaceVariant',
  fontSize: '13',
  lineHeight: '20',
  m: '0',
  ...lineClamp('2'),
});

export const subRow = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '12',
  gap: '1_5',
  lineHeight: '16',
});

export const subSeparator = css({
  color: 'outline/40',
});

export const playedTag = css({
  color: 'tertiary',
  fontWeight: 'semibold',
});

export const progressTag = css({
  fontVariantNumeric: 'tabular-nums',
});

export const playButton = css({
  borderRadius: 'full',
  flex: 'none',
});

export const playIcon = css({
  fill: '[currentColor]',
  height: '4',
  width: '4',
});

export const spinner = css({
  animation: '[spin 1s {easings.linear} infinite]',
  height: '4',
  width: '4',
});

export const skeletonList = css({
  display: 'grid',
  gap: '3',
  minWidth: '[0]',
});

export const skeletonRow = css({
  animation: pulse,
  bg: 'surfaceContainer/40',
  borderRadius: '2xl',
  height: '24',
});
