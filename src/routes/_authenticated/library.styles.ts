import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]';

export const stack = css({
  display: 'grid',
  gap: '6',
});

export const skeletonRow = css({
  display: 'grid',
  gap: '3',
});

export const skeletonTitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'md',
  height: '6',
  width: '[11rem]',
});

export const skeletonGrid = css({
  display: 'grid',
  columnGap: '3',
  rowGap: '4',
  sm: {
    gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
  },
  xl: {
    gridTemplateColumns: '[repeat(4, minmax(0, 1fr))]',
  },
  '2xl': {
    gridTemplateColumns: '[repeat(6, minmax(0, 1fr))]',
  },
});

export const skeletonArtwork = css({
  animation: pulse,
  bg: 'surfaceContainerLowest/60',
  borderBottomColor: 'outlineVariant',
  borderBottomStyle: 'solid',
  borderBottomWidth: '1px',
});

export const skeletonAspect = {
  poster: css({ aspectRatio: '[2 / 3]' }),
  video: css({ aspectRatio: '[16 / 9]' }),
};

export const skeletonBody = css({
  display: 'grid',
  gap: '2',
  pb: '3',
  pt: '2',
  px: '4',
});

export const skeletonLine = {
  title: css({
    animation: pulse,
    bg: 'surfaceContainerHigh/80',
    borderRadius: 'md',
    height: '4',
    width: '[80%]',
  }),
  subtitle: css({
    animation: pulse,
    bg: 'surfaceContainerHigh/60',
    borderRadius: 'md',
    height: '3',
    width: '[60%]',
  }),
};
