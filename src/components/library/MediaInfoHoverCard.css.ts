import { atomic } from '@jellypilot/atomic-css';
import { globalStyle, style } from '@vanilla-extract/css';

export const hoverRoot = style({});

globalStyle(`${hoverRoot} [data-part="content"]`, {
  top: 'auto',
  bottom: 'calc(100% + var(--jellypilot-space-2_5))',
  left: '0',
});

export const content = style([
  atomic({
    display: 'grid',
    gap: 2,
  }),
]);

export const title = style([
  atomic({
    display: 'grid',
    gap: 0,
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 'var(--jellypilot-line-height-20)',
    fontWeight: 'var(--jellypilot-font-weight-semibold)',
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 2,
  },
]);

export const meta = style([
  atomic({
    color: 'var(--jellypilot-color-on-surface-variant)',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    fontVariantNumeric: 'tabular-nums',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const genres = atomic({
  display: 'flex',
  flexWrap: 'wrap',
  gap: 1,
});

export const genre = style([
  atomic({
    borderRadius: 'full',
    px: 2,
    py: 0.5,
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    background: 'var(--jellypilot-color-surface-container-highest)',
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const overview = style([
  atomic({
    display: 'block',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    display: '-webkit-box',
    overflow: 'hidden',
    WebkitBoxOrient: 'vertical',
    WebkitLineClamp: 3,
  },
]);

export const progressTrack = style([
  atomic({
    overflow: 'hidden',
    width: 'full',
    height: 1,
    borderRadius: 'full',
  }),
  {
    background: 'var(--jellypilot-color-surface-container-highest)',
  },
]);

globalStyle(`${progressTrack} [data-part="fill"]`, {
  background: 'var(--jellypilot-color-secondary)',
});

export const watchedText = style([
  atomic({
    mt: 1,
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    fontVariantNumeric: 'tabular-nums',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const states = style([
  atomic({
    display: 'flex',
    flexWrap: 'wrap',
    gap: 3,
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    alignItems: 'flex-start',
    letterSpacing: '0.05em',
    paddingTop: 'var(--jellypilot-space-0_5)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    textTransform: 'uppercase',
  },
]);

export const state = atomic({
  display: 'flex',
  alignItems: 'center',
  gap: 1,
});

export const played = style({
  color: 'var(--jellypilot-color-tertiary)',
});

export const favorite = style({
  color: 'var(--jellypilot-color-secondary)',
});

export const icon = style({
  height: 'var(--jellypilot-space-3_5)',
  width: 'var(--jellypilot-space-3_5)',
});

export const popover = style([
  atomic({
    borderRadius: '2xl',
    p: 4,
  }),
  {
    background: 'var(--jellypilot-color-surface-container-low)',
    border: '1px solid var(--jellypilot-color-outline-variant)',
    boxShadow: '0 12px 36px rgb(0 0 0 / 0.16)',
    display: 'grid',
    gap: 'var(--jellypilot-space-2)',
    maxWidth: 'min(90vw, 24rem)',
    width: '20rem',
    zIndex: 100,
  },
]);

export const loading = style([
  atomic({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
    py: 3,
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);

export const spinner = style({
  height: 'var(--jellypilot-space-4)',
  width: 'var(--jellypilot-space-4)',
});

export const error = style([
  atomic({
    py: 2,
    textAlign: 'center',
  }),
  {
    color: 'var(--jellypilot-color-error)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontWeight: 'var(--jellypilot-font-weight-bold)',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
  },
]);
