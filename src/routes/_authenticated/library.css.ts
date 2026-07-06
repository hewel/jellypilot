import { style, styleVariants } from '@vanilla-extract/css';

import { vars } from '../../styles/vars.css';

const pulse = {
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
};

export const stack = style({
  display: 'grid',
  gap: vars.space['6'],
});

export const skeletonRow = style({
  display: 'grid',
  gap: vars.space['3'],
});

export const skeletonTitle = style({
  ...pulse,
  background: `color-mix(in srgb, ${vars.color.surfaceContainerHigh} 70%, transparent)`,
  borderRadius: vars.borderRadius.md,
  height: vars.space['6'],
  width: '11rem',
});

export const skeletonGrid = style({
  display: 'grid',
  gap: vars.space['3'],
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
    },
    'screen and (min-width: 1280px)': {
      gridTemplateColumns: 'repeat(4, minmax(0, 1fr))',
    },
    'screen and (min-width: 1536px)': {
      gridTemplateColumns: 'repeat(6, minmax(0, 1fr))',
    },
  },
});

export const skeletonArtwork = style([
  {
    ...pulse,
    background: `color-mix(in srgb, ${vars.color.surfaceContainerLowest} 60%, transparent)`,
    borderBottom: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const skeletonAspect = styleVariants({
  poster: {
    aspectRatio: '2 / 3',
  },
  video: {
    aspectRatio: '16 / 9',
  },
});

export const skeletonBody = style({
  display: 'grid',
  gap: vars.space['2'],
  padding: `${vars.space['2']} ${vars.space['4']} ${vars.space['3']}`,
});

export const skeletonLine = styleVariants({
  title: {
    ...pulse,
    background: `color-mix(in srgb, ${vars.color.surfaceContainerHigh} 80%, transparent)`,
    borderRadius: vars.borderRadius.md,
    height: vars.space['4'],
    width: '80%',
  },
  subtitle: {
    ...pulse,
    background: `color-mix(in srgb, ${vars.color.surfaceContainerHigh} 60%, transparent)`,
    borderRadius: vars.borderRadius.md,
    height: vars.space['3'],
    width: '60%',
  },
});
