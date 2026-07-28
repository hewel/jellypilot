import { css } from '@styled-system/css';

const pulse = '[pulse 1.8s {easings.inOut} infinite]';

export const seasonBar = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
  justifyContent: 'space-between',
});

export const seasonTabs = css({
  alignItems: 'center',
  bg: 'surfaceContainerLow/60',
  borderColor: 'outlineVariant/50',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'inline-flex',
  gap: '1',
  maxWidth: '[100%]',
  overflowX: 'auto',
  p: '1',
});

export const seasonTab = css({
  appearance: 'none',
  bg: '[transparent]',
  border: 'none',
  borderRadius: 'full',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  flexShrink: '[0]',
  fontSize: '14',
  fontWeight: 'semibold',
  lineHeight: '20',
  px: '4',
  py: '1_5',
  transitionDuration: '150',
  transitionProperty: '[background-color, color]',
  whiteSpace: 'nowrap',
  _hover: {
    bg: 'surfaceContainerHigh/80',
    color: 'onSurface',
  },
  _focusVisible: {
    outline: '[2px solid {colors.secondary}]',
    outlineOffset: '1',
  },
  _disabled: {
    cursor: 'wait',
    opacity: '[0.6]',
  },
});

export const activeSeasonTab = css({
  bg: 'primary',
  color: 'onPrimary',
  _hover: {
    bg: 'primary',
    color: 'onPrimary',
  },
});

export const seasonSelectWrap = css({
  maxWidth: '[20rem]',
});

export const seasonMeta = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
});

export const episodeList = css({
  animation: '[fadeIn 300ms {easings.emphasized} forwards]',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
});

export const episodeRow = css({
  alignItems: 'center',
  bg: 'surfaceContainerLow/60',
  borderColor: 'outlineVariant/50',
  borderRadius: 'xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'grid',
  gap: '4',
  gridTemplateColumns: '[auto minmax(0, 1fr) auto]',
  px: '5',
  py: '4',
  transitionDuration: '150',
  transitionProperty: '[border-color]',
  _hover: {
    borderColor: 'secondary/40',
  },
  sm: {
    gridTemplateColumns: '[auto minmax(0, 1fr) auto auto]',
  },
});

export const episodeNumber = css({
  color: 'onSurfaceVariant/50',
  fontSize: '24',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'bold',
  lineHeight: '32',
  minWidth: '[2.5rem]',
  textAlign: 'center',
});

export const episodeCopy = css({
  display: 'grid',
  gap: '1',
  minWidth: '[0]',
});

export const episodeTitle = css({
  color: 'onSurface',
  display: 'block',
  fontSize: '16',
  fontWeight: 'semibold',
  lineHeight: '24',
  overflow: 'hidden',
  textDecoration: 'none',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
  _hover: {
    textDecoration: 'underline',
  },
});

export const episodeSub = css({
  alignItems: 'center',
  color: 'onSurfaceVariant/70',
  display: 'flex',
  flexWrap: 'wrap',
  fontSize: '12',
  gap: '1_5',
  lineHeight: '16',
});

export const episodeRuntime = css({
  color: 'onSurfaceVariant/80',
  display: 'none',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
  whiteSpace: 'nowrap',
  sm: {
    display: 'block',
  },
});

export const skeletonRow = css({
  animation: pulse,
  bg: 'surfaceContainerLow/60',
  borderRadius: 'xl',
  height: '[72px]',
});
