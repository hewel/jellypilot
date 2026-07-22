import { css } from '@styled-system/css';

export const stack = css({
  display: 'grid',
  gap: '3',
});

export const profile = css({
  borderRadius: '2xl',
  p: '4',
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
});

export const activeProfile = css({
  borderColor: 'secondary/70',
  boxShadow: '[0 0 0 1px {colors.secondary/25}]',
});

export const warningProfile = css({
  borderColor: 'warning/60',
});

export const profileInner = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  sm: {
    alignItems: 'flex-start',
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
});

export const copy = css({
  minWidth: '[0]',
});

export const titleRow = css({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '2',
});

export const name = css({
  color: 'onSurface',
  fontSize: '15',
  lineHeight: '22',
  fontWeight: 'bold',
});

export const pill = css({
  borderRadius: 'full',
  px: '2',
  py: '0_5',
  fontSize: '10',
  lineHeight: '14',
  fontWeight: 'bold',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  color: 'onSurfaceVariant',
  letterSpacing: '8',
  textTransform: 'uppercase',
});

export const activePill = css({
  bg: 'secondary/15',
  borderColor: '[transparent]',
  color: 'secondary',
});

export const url = css({
  mt: '1',
  color: 'secondary',
  fontFamily: 'mono',
  fontSize: '12',
  lineHeight: '16',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const user = css({
  mt: '1',
  display: 'flex',
  alignItems: 'center',
  gap: '1_5',
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
});

export const warning = css({
  mt: '2',
  display: 'flex',
  alignItems: 'flex-start',
  gap: '1_5',
  color: 'warning',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
});

export const warningIcon = css({
  flexShrink: '[0]',
  height: '3_5',
  mt: '0_5',
  width: '3_5',
});

export const actions = css({
  display: 'flex',
  flexShrink: '[0]',
  flexWrap: 'wrap',
  gap: '2',
});

export const footer = css({
  mt: '5',
});

export const dangerButton = css({
  borderColor: 'error/55',
  color: 'error',
  _hover: {
    bg: 'error/10',
    borderColor: 'error',
  },
});

export const icon3_5 = css({
  height: '3_5',
  width: '3_5',
});

export const icon4_5 = css({
  height: 'lg',
  width: 'lg',
});
