import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

const lineClamp = (lines: string) =>
  ({
    display: '[-webkit-box]',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: `[${lines}]`,
  }) satisfies Record<string, string>;

/* Shallow cinematic surface: 16:5-like band that never crowds out the first
 * row at the supported 1280×720 floor. Narrow layouts stack actions, so the
 * band keeps 15rem as a floor and grows with its clamped copy instead of
 * clipping it; `sm` and up pin the fixed clamp with row actions. */
const heroSurface = {
  borderRadius: '2xl',
  boxShadow: 'lg',
  minHeight: '[15rem]',
  minWidth: '[0]',
  outline: '[1px solid rgb(255 255 255 / 0.1)]',
  outlineOffset: '[-1px]',
  overflow: 'hidden',
  position: 'relative',
  sm: {
    height: '[clamp(15rem, 24vw, 20rem)]',
  },
} as const;

export const hero = css(heroSurface);

export const artwork = css({
  inset: '[0]',
  position: 'absolute',
});

export const image = css({
  height: 'full',
  objectFit: 'cover',
  width: 'full',
});

export const imageFallback = css({
  backgroundImage:
    '[linear-gradient(135deg, {colors.surfaceContainerHigh} 0%, {colors.surfaceContainerLowest} 100%)]',
  height: 'full',
  width: 'full',
});

/* Layered token scrims keep headline, metadata, and actions readable over
 * bright and dark artwork alike. */
export const scrim = css({
  backgroundImage:
    '[linear-gradient(to right, {colors.surface/92} 0%, {colors.surface/62} 42%, {colors.surface/18} 68%, transparent 88%), linear-gradient(to top, {colors.surface/72} 0%, transparent 45%)]',
  inset: '[0]',
  position: 'absolute',
});

export const content = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '1_5',
  justifyContent: 'flex-end',
  minHeight: '[15rem]',
  minWidth: '[0]',
  p: '4',
  position: 'relative',
  sm: {
    gap: '2',
    inset: '[0]',
    minHeight: '[0]',
    p: '6',
    position: 'absolute',
  },
});

export const eyebrow = css({
  color: 'secondary',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '8',
  lineHeight: '16',
  textTransform: 'uppercase',
});

export const headline = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '24',
  fontWeight: 'bold',
  lineHeight: '32',
  textWrap: 'balance',
  ...lineClamp('2'),
  sm: {
    fontSize: '32',
    lineHeight: '40',
    WebkitLineClamp: '[1]',
  },
});

export const metadata = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '20',
  ...lineClamp('1'),
});

export const overview = css({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
  maxWidth: '[44rem]',
  ...lineClamp('2'),
  lg: {
    WebkitLineClamp: '[3]',
  },
});

export const actions = css({
  alignItems: 'stretch',
  display: 'grid',
  gap: '2',
  pt: '1',
  sm: {
    alignItems: 'center',
    display: 'flex',
  },
});

export const actionIcon = css({
  height: '4',
  width: '4',
});

/* Owner-local outlined button treatment for the Details router link; mirrors
 * the design-system outlined Button without bypassing router navigation. */
export const detailsLink = css({
  alignItems: 'center',
  backdropFilter: '[blur(8px)]',
  bg: 'surface/40',
  borderColor: 'outline',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurface',
  display: 'inline-flex',
  fontSize: '14',
  fontWeight: 'bold',
  gap: '2',
  justifyContent: 'center',
  lineHeight: '20',
  minHeight: '11',
  padding: '[0.875em 1.125em]',
  textDecoration: 'none',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, color, box-shadow, transform]',
  userSelect: 'none',
  _hover: {
    bg: 'primary/10',
    borderColor: 'primary',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  _focusVisible: {
    outline: '[2px solid {colors.primary}]',
    outlineOffset: '[2px]',
  },
});

export const skeletonHero = css(heroSurface);

export const skeletonContent = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  justifyContent: 'flex-end',
  minHeight: '[15rem]',
  p: '4',
  position: 'relative',
  sm: {
    inset: '[0]',
    minHeight: '[0]',
    p: '6',
    position: 'absolute',
  },
});

export const skeletonEyebrow = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/55',
  borderRadius: 'md',
  height: '3',
  width: '16',
});

export const skeletonHeadline = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/70',
  borderRadius: 'md',
  height: '8',
  width: '[55%]',
});

export const skeletonLine = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/60',
  borderRadius: 'md',
  height: '4',
  width: '[80%]',
});

export const skeletonActions = css({
  animation: pulse,
  bg: 'surfaceContainerHigh/55',
  borderRadius: '2xl',
  height: '11',
  width: '[14rem]',
});
