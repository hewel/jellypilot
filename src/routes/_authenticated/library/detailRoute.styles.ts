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
  gap: '8',
  minWidth: '[0]',
  width: 'full',
});

/* Below-hero content (season bar, episode rows) owns the standard gutter
 * because the shell drops its padding on detail pages. */
export const contentSection = css({
  display: 'grid',
  gap: '6',
  minWidth: '[0]',
  px: '4',
  width: 'full',
});

export const sectionHeading = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '22',
  fontWeight: 'bold',
  letterSpacing: '[-0.01em]',
  lineHeight: '28',
  m: '0',
});

export const pillButton = css({
  borderRadius: 'full',
  maxWidth: '[100%]',
});

export const playGlow = css({
  boxShadow: '[0 8px 24px {colors.primary/40}]',
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
  px: '4',
});

export const skeletonBar = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'full',
  height: '9',
  width: '[16rem]',
});
