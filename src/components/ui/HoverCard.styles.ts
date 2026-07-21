import { css } from '@styled-system/css';

/** Floating card chrome; positioned inline by the HoverCard primitive. */
export const card = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderColor: 'outlineVariant',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  maxWidth: '[min(90vw, 24rem)]',
  p: '4',
  width: '[20rem]',
  zIndex: '100',
});
