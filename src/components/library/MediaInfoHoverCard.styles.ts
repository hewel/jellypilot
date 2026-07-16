import { css } from '@styled-system/css';

const titleClamp = {
  display: '[-webkit-box]',
  WebkitBoxOrient: 'vertical',
  WebkitLineClamp: '[2]',
} satisfies Record<string, string>;

const overviewClamp = {
  display: '[-webkit-box]',
  WebkitBoxOrient: 'vertical',
  WebkitLineClamp: '[3]',
} satisfies Record<string, string>;

export const content = css({
  display: 'grid',
  gap: '2',
});

export const title = css({
  color: 'onSurface',
  ...titleClamp,
  fontSize: '14',
  fontWeight: 'semibold',
  lineHeight: '20',
  overflow: 'hidden',
});

export const meta = css({
  color: 'onSurfaceVariant',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'bold',
  letterSpacing: '[0.05em]',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const genres = css({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '1',
});

export const genre = css({
  bg: 'surfaceContainerHighest/70',
  borderRadius: 'full',
  color: 'onSurfaceVariant',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  px: '2',
  py: '0_5',
  textTransform: 'uppercase',
});

export const overview = css({
  color: 'onSurfaceVariant/90',
  ...overviewClamp,
  fontSize: '12',
  lineHeight: '16',
  overflow: 'hidden',
});

export const progressTrack = css({
  bg: 'surfaceContainerHighest/70',
  borderRadius: 'full',
  height: '1',
  overflow: 'hidden',
  width: 'full',
});

export const progressBar = css({
  bg: 'secondary',
  height: 'full',
});

export const watchedText = css({
  color: 'onSurfaceVariant',
  fontSize: '11',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  mt: '1',
  textTransform: 'uppercase',
});

export const states = css({
  color: 'onSurfaceVariant',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '12',
  fontWeight: 'bold',
  gap: '3',
  letterSpacing: '[0.05em]',
  lineHeight: '16',
  pt: '0_5',
  textTransform: 'uppercase',
});

export const state = css({
  alignItems: 'center',
  display: 'flex',
  gap: '1',
});

export const played = css({
  color: 'tertiary',
});

export const favorite = css({
  color: 'secondary',
});

export const icon = css({
  height: '3_5',
  width: '3_5',
});

export const popover = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderColor: 'outlineVariant',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  maxWidth: '[min(90vw, 24rem)]',
  p: '4',
  width: '[20rem]',
  zIndex: '100',
});

export const loading = css({
  alignItems: 'center',
  color: 'onSurfaceVariant',
  display: 'flex',
  fontSize: '12',
  fontWeight: 'bold',
  gap: '2',
  justifyContent: 'center',
  letterSpacing: '[0.05em]',
  lineHeight: '16',
  py: '3',
  textTransform: 'uppercase',
});

export const spinner = css({
  animation: '[spin 1s linear infinite]',
  height: '4',
  width: '4',
});

export const error = css({
  color: 'error/90',
  fontSize: '12',
  fontWeight: 'bold',
  letterSpacing: '[0.05em]',
  lineHeight: '16',
  py: '2',
  textAlign: 'center',
  textTransform: 'uppercase',
});
