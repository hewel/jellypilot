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
