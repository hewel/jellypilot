import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

export const stack = css({
  display: 'grid',
  gap: '8',
});

/* Sticky Home search chrome. The glass capsule belongs to the search bar
 * itself; this host is only a positioning rail plus a top-fade scrim, so the
 * row is invisible over the ambient background at rest and scrolled rows
 * dissolve beneath the control instead of clipping against a bordered strip. */
export const homeSearch = css({
  alignItems: 'center',
  backgroundImage: '[linear-gradient(to bottom, {colors.background/90} 30%, transparent)]',
  display: 'flex',
  justifyContent: 'center',
  mb: '6',
  pb: '3',
  position: 'sticky',
  pt: '2',
  top: '0',
  zIndex: '40',
});

export const skeletonRow = css({
  display: 'grid',
  gap: '4',
});

export const skeletonHeader = css({
  alignItems: 'center',
  display: 'flex',
  justifyContent: 'space-between',
});

export const skeletonTitle = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'md',
  height: '7',
  width: '[14rem]',
});

export const skeletonAction = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/55',
  borderRadius: 'md',
  height: '5',
  width: '14',
});

export const skeletonGrid = {
  poster: css({
    columnGap: '4',
    display: 'grid',
    gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
    rowGap: '6',
    sm: {
      gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
    },
    xl: {
      gridTemplateColumns: '[repeat(4, minmax(0, 1fr))]',
    },
    '2xl': {
      gridTemplateColumns: '[repeat(5, minmax(0, 1fr))]',
    },
  }),
  video: css({
    columnGap: '6',
    display: 'grid',
    gridTemplateColumns: '[minmax(0, 1fr)]',
    rowGap: '6',
    sm: {
      gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
    },
    xl: {
      gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
    },
  }),
};

export const skeletonCard = css({
  minWidth: '[0]',
});

export const skeletonArtwork = css({
  animation: pulse,
  bg: 'surfaceContainerLowest/60',
  borderRadius: '2xl',
  boxShadow: 'md',
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  outlineOffset: '[-1px]',
});

export const skeletonAspect = {
  poster: css({ aspectRatio: '[2 / 3]' }),
  video: css({ aspectRatio: '[16 / 9]' }),
};

export const skeletonBody = css({
  display: 'grid',
  gap: '1',
  pt: '3',
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
