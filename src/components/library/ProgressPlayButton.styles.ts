import { css } from '@styled-system/css';

/* Primary play action with an optional watched-progress fill. The fill is a
 * background gradient driven by the --play-progress custom property so it
 * layers over the button background without competing with Button layout.
 * Sizing lands between Button sm and md — a hero-specific intermediate that
 * does not belong on the shared recipe. */
export const root = css({
  borderRadius: 'full',
  fontSize: '13',
  justifyContent: 'flex-start',
  lineHeight: '[1.125rem]',
  maxWidth: '[100%]',
  minHeight: '[2.625rem]',
  overflow: 'hidden',
  padding: '[0.7em 1em]',
});

export const glow = css({
  boxShadow: '[0 8px 24px {colors.primary/40}]',
  minWidth: '[16rem]',
});

export const progressPrimary = css({
  backgroundImage:
    '[linear-gradient(to right, {colors.background/38} var(--play-progress), transparent var(--play-progress))]',
});

export const progressOutlined = css({
  backgroundImage:
    '[linear-gradient(to right, {colors.onSurface/12} var(--play-progress), transparent var(--play-progress))]',
});

export const text = css({
  display: 'grid',
  gap: '0_5',
  justifyItems: 'start',
  textAlign: 'left',
});

export const remaining = css({
  fontSize: '11',
  fontWeight: 'semibold',
  lineHeight: '14',
  opacity: '[0.85]',
});
