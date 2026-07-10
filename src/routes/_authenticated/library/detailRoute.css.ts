import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const stack = style([
  atomic({
    display: 'grid',
    gap: 6,
  }),
]);

export const content = style([
  atomic({
    display: 'grid',
    gap: 6,
    width: 'full',
  }),
  {
    marginInline: 'auto',
    maxWidth: '1400px',
    padding: 'var(--jellypilot-space-6) var(--jellypilot-space-6)',
    '@media': {
      'screen and (min-width: 1024px)': {
        paddingInline: 'var(--jellypilot-space-10)',
      },
      'screen and (min-width: 1280px)': {
        paddingInline: 'var(--jellypilot-space-12)',
      },
    },
  },
]);

export const overview = style({
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 'var(--jellypilot-line-height-22)',
  maxWidth: '1100px',
  '@media': {
    'screen and (min-width: 1024px)': {
      fontSize: 'var(--jellypilot-font-size-15)',
      lineHeight: 'var(--jellypilot-line-height-24)',
    },
  },
});

export const pillRow = style([
  atomic({
    display: 'flex',
    wrap: 'wrap',
    gap: 2,
  }),
]);

export const genre = style([
  atomic({
    borderRadius: 'full',
    fontWeight: 'bold',
  }),
  {
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    color: mix('var(--jellypilot-color-on-surface-variant)', 0.9),
    fontSize: 'var(--jellypilot-font-size-11)',
    letterSpacing: '0.08em',
    lineHeight: 'var(--jellypilot-line-height-16)',
    padding: 'var(--jellypilot-space-1) var(--jellypilot-space-3)',
    textTransform: 'uppercase',
  },
]);

export const pillButton = style([
  atomic({
    rounded: 'full',
  }),
]);

export const playIcon = style([
  atomic({
    height: 4,
    width: 4,
  }),
  {
    fill: 'currentColor',
  },
]);

export const icon4 = style([
  atomic({
    height: 4,
    width: 4,
  }),
]);

export const icon6 = style([
  atomic({
    height: 6,
    width: 6,
  }),
]);

export const spinner = style({
  animation: 'spin 1s linear infinite',
});

export const error = style({
  color: 'var(--jellypilot-color-error)',
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  paddingInline: 'var(--jellypilot-space-6)',
});

export const skeletonHero = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix('var(--jellypilot-color-surface-container-lowest)', 0.6),
  height: 'clamp(280px, 44vh, 560px)',
});

export const skeletonContent = style([
  atomic({
    display: 'grid',
    gap: 4,
    width: 'full',
  }),
  {
    marginInline: 'auto',
    maxWidth: '1400px',
    padding: 'var(--jellypilot-space-2) var(--jellypilot-space-6)',
    '@media': {
      'screen and (min-width: 1024px)': {
        paddingInline: 'var(--jellypilot-space-10)',
      },
      'screen and (min-width: 1280px)': {
        paddingInline: 'var(--jellypilot-space-12)',
      },
    },
  },
]);

export const skeletonLine = style([
  atomic({
    borderRadius: 'md',
    height: 4,
    width: 'full',
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.6),
    maxWidth: '1100px',
  },
]);

export const skeletonLineShort = style({
  maxWidth: '900px',
  width: '83.333%',
});

export const skeletonPill = style([
  atomic({
    borderRadius: 'full',
    height: 7,
    width: 24,
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.7),
  },
]);

export const section = style([
  atomic({
    display: 'grid',
    gap: 4,
  }),
]);

export const sectionCompact = style([
  atomic({
    display: 'grid',
    gap: 3,
  }),
]);

export const sectionHeader = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 2,
  }),
  {
    '@media': {
      'screen and (min-width: 640px)': {
        alignItems: 'flex-end',
        flexDirection: 'row',
        justifyContent: 'space-between',
      },
    },
  },
]);

export const sectionTitle = style([
  atomic({
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-22)',
    lineHeight: 'var(--jellypilot-line-height-28)',
  },
]);

export const titleSmall = style([
  atomic({
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 'var(--jellypilot-line-height-24)',
  },
]);

export const sectionSubtitle = style({
  color: mix('var(--jellypilot-color-on-surface-variant)', 0.8),
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
  fontVariantNumeric: 'tabular-nums',
});

export const fadeList = style([
  atomic({
    display: 'flex',
    flexDirection: 'column',
    gap: 3,
  }),
  {
    animation: 'fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards',
  },
]);

export const seasonTabs = style([
  atomic({
    display: 'flex',
    gap: 2,
    overflowX: 'auto',
    rounded: '2xl',
  }),
  {
    background: mix('var(--jellypilot-color-surface-container-low)', 0.7),
    border: `1px solid var(--jellypilot-color-outline-variant)`,
    padding: 'var(--jellypilot-space-2)',
  },
]);

export const seasonItem = style({
  flexShrink: 0,
});

export const selectedSeason = style({
  background: mix('var(--jellypilot-color-secondary-container)', 0.45),
  borderColor: 'var(--jellypilot-color-secondary)',
  color: 'var(--jellypilot-color-on-secondary-container)',
});

export const selectWrap = style({
  maxWidth: '20rem',
});

export const episodeCard = style([
  atomic({
    alignItems: 'center',
    display: 'grid',
    gap: 4,
    gridTemplateColumns: '1fr',
  }),
  {
    padding: 'var(--jellypilot-space-3) !important',
    '@media': {
      'screen and (min-width: 640px)': {
        gridTemplateColumns: '160px minmax(0, 1fr) auto',
      },
      'screen and (min-width: 1024px)': {
        gridTemplateColumns: '220px minmax(0, 1fr) auto',
      },
    },
  },
]);

export const episodeImageWrap = style([
  atomic({
    overflow: 'hidden',
    rounded: 'lg',
    display: 'none',
  }),
  {
    aspectRatio: '16 / 9',
    background: mix('var(--jellypilot-color-surface-container-lowest)', 0.6),
    width: '160px',
    '@media': {
      'screen and (min-width: 640px)': {
        display: 'block',
      },
      'screen and (min-width: 1024px)': {
        width: '220px',
      },
    },
  },
]);

export const episodeFallback = style([
  atomic({
    alignItems: 'center',
    display: 'flex',
    fontWeight: 'bold',
    justify: 'center',
  }),
  {
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-11)',
    height: '100%',
    letterSpacing: '0.08em',
    lineHeight: 'var(--jellypilot-line-height-16)',
    textTransform: 'uppercase',
  },
]);

export const image = style({
  height: '100%',
  objectFit: 'cover',
  outline: '1px solid rgb(255 255 255 / 0.1)',
  outlineOffset: '-1px',
  width: '100%',
});

export const episodeCopy = style([
  atomic({
    display: 'grid',
    gap: 1.5,
  }),
  {
    minWidth: 0,
  },
]);

export const episodeMeta = style([
  atomic({
    alignItems: 'center',
    display: 'flex',
    wrap: 'wrap',
    gap: 2,
  }),
]);

export const episodeLabel = style([
  atomic({
    fontWeight: 'bold',
  }),
  {
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-11)',
    letterSpacing: '0.08em',
    lineHeight: 'var(--jellypilot-line-height-16)',
    textTransform: 'uppercase',
  },
]);

export const muted = style({
  color: mix('var(--jellypilot-color-on-surface-variant)', 0.7),
  fontSize: 'var(--jellypilot-font-size-12)',
  lineHeight: 'var(--jellypilot-line-height-16)',
});

export const progressText = style([
  atomic({
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 'var(--jellypilot-line-height-16)',
    fontVariantNumeric: 'tabular-nums',
  },
]);

export const episodeLink = style([
  atomic({
    display: 'block',
    fontWeight: 'semibold',
  }),
  {
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 'var(--jellypilot-line-height-24)',
    overflow: 'hidden',
    textDecoration: 'none',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
    selectors: {
      '&:hover': {
        textDecoration: 'underline',
      },
    },
  },
]);

export const actionCell = style([
  atomic({
    display: 'flex',
  }),
  {
    flexShrink: 0,
  },
]);

export const episodeButton = style([
  atomic({
    rounded: 'full',
    px: 5,
    py: 2,
  }),
  {
    fontSize: 'var(--jellypilot-font-size-14)',
    fontWeight: 'semibold',
    lineHeight: 'var(--jellypilot-line-height-20)',
    letterSpacing: 0,
    textTransform: 'uppercase',
  },
]);

export const skeletonTitle = style([
  atomic({
    borderRadius: 'md',
    height: 6,
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.7),
    width: '11rem',
  },
]);

export const skeletonEpisodeImage = style([
  episodeImageWrap,
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  },
]);

export const skeletonButton = style([
  atomic({
    borderRadius: 'full',
    height: 10,
    width: 24,
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-primary-container)', 0.4),
  },
]);

export const skeletonMiniLine = style([
  atomic({
    borderRadius: 'md',
    height: 3,
    width: 14,
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.7),
  },
]);

export const skeletonSmallPill = style([
  atomic({
    borderRadius: 'full',
    height: 6,
    width: 20,
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.6),
  },
]);

export const skeletonEpisodeTitle = style([
  atomic({
    borderRadius: 'md',
    height: 5,
  }),
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
    background: mix('var(--jellypilot-color-surface-container-high)', 0.8),
    width: '80%',
  },
]);
