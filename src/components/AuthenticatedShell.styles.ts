import { css } from '@styled-system/css';

export const main = css({
  animation: '[fadeIn 300ms {easings.emphasized} forwards]',
  color: 'onSurface',
  display: 'flex',
  flexDirection: 'column',
  mx: 'auto',
  pb: '[10rem]',
  width: 'full',
});

export const floatingControls = css({
  alignItems: 'center',
  backdropFilter: '[blur(24px)]',
  bg: 'surfaceContainerLow/80',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/40',
  borderRadius: '3xl',
  bottom: '4',
  boxShadow: '2xl',
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  p: '1',
  position: 'fixed',
  right: '4',
  zIndex: '100',
});
