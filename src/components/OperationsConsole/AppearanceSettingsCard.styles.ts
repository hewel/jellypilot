import { css } from '@styled-system/css';

export const stack = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '4',
  minWidth: '[0]',
});

export const saving = css({
  display: 'flex',
  alignItems: 'center',
  gap: '1_5',
  color: 'secondary',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
});

export const pingDot = css({
  animation: '[ping 1s cubic-bezier(0, 0, 0.2, 1) infinite]',
  bg: 'secondary',
  borderRadius: 'full',
  height: '1_5',
  width: '1_5',
});
