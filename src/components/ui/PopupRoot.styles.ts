import { css } from '@styled-system/css';

/** Full-viewport layer; children opt into pointer events. */
export const root = css({
  inset: '[0]',
  pointerEvents: 'none',
  position: 'fixed',
  zIndex: '100',
});
