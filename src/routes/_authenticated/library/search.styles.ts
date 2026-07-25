import { css } from '@styled-system/css';

export const root = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '4',
  minWidth: '[0]',
});

export const header = css({
  alignItems: 'baseline',
  columnGap: '3',
  display: 'flex',
  flexWrap: 'wrap',
  rowGap: '1',
});

export const heading = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const count = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
});

export const results = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '1',
  listStyle: 'none',
  margin: '[0]',
  padding: '[0]',
});

export const resultItem = css({
  minWidth: '[0]',
});

export const statusActions = css({
  display: 'flex',
  justifyContent: 'center',
  pt: '2',
});

export const loadMoreError = css({
  alignItems: 'center',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  pt: '2',
});

export const error = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  textAlign: 'center',
});

export const icon4 = css({
  height: '4',
  width: '4',
});

export const spin = css({
  animation: '[spin 1s {easings.linear} infinite]',
});

export const liveStatus = css({
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
  minHeight: '4',
  textAlign: 'center',
});

export const sentinel = css({
  height: '[1px]',
  width: 'full',
});
