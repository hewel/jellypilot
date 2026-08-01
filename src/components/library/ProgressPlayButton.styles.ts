import { css } from '@styled-system/css';

/* Primary play action with an optional watched-progress fill. The fill is a
 * background gradient driven by the --play-progress custom property so it
 * layers over the button background without competing with Button layout. */
export const root = css({
  borderRadius: 'full',
  justifyContent: 'flex-start',
  maxWidth: '[100%]',
  overflow: 'hidden',
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
