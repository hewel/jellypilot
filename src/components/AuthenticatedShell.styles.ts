import { css } from '@styled-system/css';

export const shell = css({
  color: 'onSurface',
  display: 'flex',
  minHeight: '[100dvh]',
});

export const main = css({
  animation: '[fadeIn 300ms {easings.emphasized} forwards]',
  color: 'onSurface',
  display: 'flex',
  flex: '1',
  flexDirection: 'column',
  minWidth: '[0]',
  mx: 'auto',
  pb: '8',
  pt: '2',
  px: '2_5',
  width: 'full',
});
