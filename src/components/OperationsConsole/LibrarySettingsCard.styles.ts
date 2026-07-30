import { css } from '@styled-system/css';

export const toggle = css({
  display: 'flex',
  alignItems: 'flex-start',
  gap: '3',
  borderRadius: '2xl',
  p: '4',
  textAlign: 'left',
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
  boxShadow: 'inner',
  cursor: 'pointer',
  _focusVisible: {
    outline: '[2px solid {colors.primary}]',
    outlineOffset: '[2px]',
  },
});

export const checkboxOffset = css({
  mt: '0_5',
});

export const checkboxChecked = css({
  bg: 'primary',
  borderColor: 'primary',
});

export const copy = css({
  minWidth: '[0]',
});

export const title = css({
  display: 'block',
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
});

export const description = css({
  mt: '1',
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
});

export const icon3_5 = css({
  height: '3_5',
  width: '3_5',
});

export const stats = css({
  mt: '3',
  px: '4',
  display: 'grid',
  gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
  gap: '3',
});

export const stat = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '0_5',
});

export const statLabel = css({
  color: 'onSurfaceVariant/70',
  fontSize: '11',
  lineHeight: '14',
  textTransform: 'uppercase',
  letterSpacing: '8',
});

export const statValue = css({
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '20',
  fontWeight: 'semibold',
  fontVariantNumeric: 'tabular-nums',
});

export const statPlaceholder = css({
  gridColumn: '[1 / -1]',
  color: 'onSurfaceVariant/70',
  fontSize: '12',
  lineHeight: '16',
});

export const statError = css({
  gridColumn: '[1 / -1]',
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
});

export const clearRow = css({
  mt: '3',
  px: '4',
  display: 'flex',
  justifyContent: 'flex-end',
});

export const dialogContent = css({
  bg: 'surface',
  color: 'onSurface',
  borderRadius: '2xl',
  boxShadow: 'xl',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
  maxWidth: '[26rem]',
  p: '6',
  outline: 'none',
});

export const dialogTitle = css({
  color: 'onSurface',
  fontSize: '18',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const dialogDescription = css({
  mt: '2',
  color: 'onSurfaceVariant/80',
  fontSize: '14',
  lineHeight: '20',
});

export const dialogActions = css({
  mt: '6',
  display: 'flex',
  justifyContent: 'flex-end',
  gap: '3',
});
