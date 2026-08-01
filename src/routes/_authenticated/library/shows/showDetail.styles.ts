import { css } from '@styled-system/css';

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
