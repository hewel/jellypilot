import { css } from '@styled-system/css';

export const backButton = css({
  bg: '[rgb(0 0 0 / 0.35)]',
  backdropFilter: '[blur(12px)]',
  borderColor: '[rgb(255 255 255 / 0.15)]',
  borderRadius: 'full',
  boxShadow: '2xl',
  color: '[#fff]',
  left: '4',
  position: 'absolute',
  top: '4',
  transitionProperty: '[background-color, border-color, transform]',
  zIndex: '[20]',
  _hover: {
    bg: '[rgb(0 0 0 / 0.5)]',
    borderColor: '[rgb(255 255 255 / 0.3)]',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  lg: {
    left: '8',
    top: '6',
  },
  xl: {
    left: '10',
  },
});
