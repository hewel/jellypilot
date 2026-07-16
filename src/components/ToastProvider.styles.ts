import { css } from '@styled-system/css';

export const viewport = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  position: 'fixed',
  zIndex: '50',
  bottom: '4',
  pointerEvents: 'none',
  right: '4',
});
