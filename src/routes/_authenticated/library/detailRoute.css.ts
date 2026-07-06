import { style } from '@vanilla-extract/css';

import { vars } from '../../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const stack = style({
  display: 'grid',
  gap: vars.space['6'],
});

export const content = style({
  display: 'grid',
  gap: vars.space['6'],
  marginInline: 'auto',
  maxWidth: '1400px',
  padding: `${vars.space['6']} ${vars.space['6']}`,
  width: '100%',
  '@media': {
    'screen and (min-width: 1024px)': {
      paddingInline: vars.space['10'],
    },
    'screen and (min-width: 1280px)': {
      paddingInline: vars.space['12'],
    },
  },
});

export const overview = style({
  color: vars.color.onSurfaceVariant,
  fontSize: vars.fontSize['14'],
  lineHeight: '1.375rem',
  maxWidth: '1100px',
  '@media': {
    'screen and (min-width: 1024px)': {
      fontSize: vars.fontSize['15'],
      lineHeight: vars.lineHeight['24'],
    },
  },
});

export const pillRow = style({
  display: 'flex',
  flexWrap: 'wrap',
  gap: vars.space['2'],
});

export const genre = style({
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius.full,
  color: mix(vars.color.onSurfaceVariant, 0.9),
  fontSize: vars.fontSize['11'],
  fontWeight: vars.fontWeight.bold,
  letterSpacing: '0.08em',
  lineHeight: vars.lineHeight['16'],
  padding: `${vars.space['1']} ${vars.space['3']}`,
  textTransform: 'uppercase',
});

export const pillButton = style({
  borderRadius: vars.borderRadius.full,
});

export const playIcon = style({
  fill: 'currentColor',
  height: vars.space['4'],
  width: vars.space['4'],
});

export const icon4 = style({
  height: vars.space['4'],
  width: vars.space['4'],
});

export const icon6 = style({
  height: vars.space['6'],
  width: vars.space['6'],
});

export const spinner = style({
  animation: 'spin 1s linear infinite',
});

export const error = style({
  color: vars.color.error,
  fontSize: vars.fontSize['12'],
  lineHeight: vars.lineHeight['16'],
  paddingInline: vars.space['6'],
});

export const skeletonHero = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerLowest, 0.6),
  height: 'clamp(280px, 44vh, 560px)',
});

export const skeletonContent = style({
  display: 'grid',
  gap: vars.space['4'],
  marginInline: 'auto',
  maxWidth: '1400px',
  padding: `${vars.space['2']} ${vars.space['6']}`,
  width: '100%',
  '@media': {
    'screen and (min-width: 1024px)': {
      paddingInline: vars.space['10'],
    },
    'screen and (min-width: 1280px)': {
      paddingInline: vars.space['12'],
    },
  },
});

export const skeletonLine = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.6),
  borderRadius: vars.borderRadius.md,
  height: vars.space['4'],
  maxWidth: '1100px',
  width: '100%',
});

export const skeletonLineShort = style({
  maxWidth: '900px',
  width: '83.333%',
});

export const skeletonPill = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.7),
  borderRadius: vars.borderRadius.full,
  height: vars.space['7'],
  width: vars.space['24'],
});

export const section = style({
  display: 'grid',
  gap: vars.space['4'],
});

export const sectionCompact = style({
  display: 'grid',
  gap: vars.space['3'],
});

export const sectionHeader = style({
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['2'],
  '@media': {
    'screen and (min-width: 640px)': {
      alignItems: 'flex-end',
      flexDirection: 'row',
      justifyContent: 'space-between',
    },
  },
});

export const sectionTitle = style({
  color: vars.color.onSurface,
  fontSize: vars.fontSize['22'],
  fontWeight: vars.fontWeight.bold,
  lineHeight: vars.lineHeight['28'],
});

export const titleSmall = style({
  color: vars.color.onSurface,
  fontSize: vars.fontSize['16'],
  fontWeight: vars.fontWeight.semibold,
  lineHeight: vars.lineHeight['24'],
});

export const sectionSubtitle = style({
  color: mix(vars.color.onSurfaceVariant, 0.8),
  fontSize: vars.fontSize['12'],
  fontVariantNumeric: 'tabular-nums',
  lineHeight: vars.lineHeight['16'],
});

export const fadeList = style({
  animation: 'fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards',
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['3'],
});

export const seasonTabs = style({
  background: mix(vars.color.surfaceContainerLow, 0.7),
  border: `1px solid ${vars.color.outlineVariant}`,
  borderRadius: vars.borderRadius['2xl'],
  display: 'flex',
  gap: vars.space['2'],
  overflowX: 'auto',
  padding: vars.space['2'],
});

export const seasonItem = style({
  flexShrink: 0,
});

export const selectedSeason = style({
  background: mix(vars.color.secondaryContainer, 0.45),
  borderColor: vars.color.secondary,
  color: vars.color.onSecondaryContainer,
});

export const selectWrap = style({
  maxWidth: '20rem',
});

export const episodeCard = style({
  alignItems: 'center',
  display: 'grid',
  gap: vars.space['4'],
  gridTemplateColumns: '1fr',
  padding: `${vars.space['3']} !important`,
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: '160px minmax(0, 1fr) auto',
    },
    'screen and (min-width: 1024px)': {
      gridTemplateColumns: '220px minmax(0, 1fr) auto',
    },
  },
});

export const episodeImageWrap = style({
  aspectRatio: '16 / 9',
  background: mix(vars.color.surfaceContainerLowest, 0.6),
  borderRadius: vars.borderRadius.lg,
  display: 'none',
  overflow: 'hidden',
  width: '160px',
  '@media': {
    'screen and (min-width: 640px)': {
      display: 'block',
    },
    'screen and (min-width: 1024px)': {
      width: '220px',
    },
  },
});

export const episodeFallback = style({
  alignItems: 'center',
  color: vars.color.onSurfaceVariant,
  display: 'flex',
  fontSize: vars.fontSize['11'],
  fontWeight: vars.fontWeight.bold,
  height: '100%',
  justifyContent: 'center',
  letterSpacing: '0.08em',
  lineHeight: vars.lineHeight['16'],
  textTransform: 'uppercase',
});

export const image = style({
  height: '100%',
  objectFit: 'cover',
  outline: '1px solid rgb(255 255 255 / 0.1)',
  outlineOffset: '-1px',
  width: '100%',
});

export const episodeCopy = style({
  display: 'grid',
  gap: vars.space['1_5'],
  minWidth: 0,
});

export const episodeMeta = style({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: vars.space['2'],
});

export const episodeLabel = style({
  color: vars.color.secondary,
  fontSize: vars.fontSize['11'],
  fontWeight: vars.fontWeight.bold,
  letterSpacing: '0.08em',
  lineHeight: vars.lineHeight['16'],
  textTransform: 'uppercase',
});

export const muted = style({
  color: mix(vars.color.onSurfaceVariant, 0.7),
  fontSize: vars.fontSize['12'],
  lineHeight: vars.lineHeight['16'],
});

export const progressText = style({
  color: vars.color.secondary,
  fontSize: vars.fontSize['12'],
  fontVariantNumeric: 'tabular-nums',
  fontWeight: vars.fontWeight.semibold,
  lineHeight: vars.lineHeight['16'],
});

export const episodeLink = style({
  color: vars.color.onSurface,
  display: 'block',
  fontSize: vars.fontSize['16'],
  fontWeight: vars.fontWeight.semibold,
  lineHeight: vars.lineHeight['24'],
  overflow: 'hidden',
  textDecoration: 'none',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  selectors: {
    '&:hover': {
      textDecoration: 'underline',
    },
  },
});

export const actionCell = style({
  display: 'flex',
  flexShrink: 0,
});

export const episodeButton = style({
  borderRadius: vars.borderRadius.full,
  fontSize: vars.fontSize['14'],
  fontWeight: vars.fontWeight.semibold,
  letterSpacing: 0,
  lineHeight: vars.lineHeight['20'],
  padding: `${vars.space['2']} ${vars.space['5']}`,
  textTransform: 'uppercase',
});

export const skeletonTitle = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.7),
  borderRadius: vars.borderRadius.md,
  height: vars.space['6'],
  width: '11rem',
});

export const skeletonEpisodeImage = style([
  episodeImageWrap,
  {
    animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  },
]);

export const skeletonButton = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.primaryContainer, 0.4),
  borderRadius: vars.borderRadius.full,
  height: vars.space['10'],
  width: vars.space['24'],
});

export const skeletonMiniLine = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.7),
  borderRadius: vars.borderRadius.md,
  height: vars.space['3'],
  width: vars.space['14'],
});

export const skeletonSmallPill = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.6),
  borderRadius: vars.borderRadius.full,
  height: vars.space['6'],
  width: vars.space['20'],
});

export const skeletonEpisodeTitle = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: mix(vars.color.surfaceContainerHigh, 0.8),
  borderRadius: vars.borderRadius.md,
  height: vars.space['5'],
  width: '80%',
});
