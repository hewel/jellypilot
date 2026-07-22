import { css } from '@styled-system/css';

/** Floating card chrome; runtime computePosition writes inline left/top overrides. */
export const card = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderColor: 'outlineVariant',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  left: '0',
  maxWidth: '[min(90vw, 24rem)]',
  p: '4',
  position: 'absolute',
  top: '0',
  width: '[20rem]',
  zIndex: '100',
});
