import { css } from '@styled-system/css';

/** The authenticated shell removes page gutters; player chrome owns its full viewport. */
export const route = css({
  backgroundColor: '[#000]',
  height: '[100dvh]',
  minHeight: '[28rem]',
  minWidth: '[0]',
  overflow: 'hidden',
  width: 'full',
});
