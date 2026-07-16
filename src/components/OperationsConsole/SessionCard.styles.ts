import { css } from '@styled-system/css';

export const card = css({
  bg: 'errorContainer/5',
  borderColor: 'error/20',
  _hover: {
    borderColor: 'error/45',
  },
});

export const signOutButton = css({
  borderColor: 'error/55',
  color: 'error',
  mt: '5',
  width: 'full',
  _hover: {
    bg: 'error/10',
    borderColor: 'error',
  },
});

export const dangerButton = css({
  borderColor: 'error/60',
  color: 'error',
  _hover: {
    bg: 'error/10',
  },
});
