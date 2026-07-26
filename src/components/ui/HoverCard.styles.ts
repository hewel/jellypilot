import { css } from '@styled-system/css';

/** Floating card chrome; runtime computePosition writes inline left/top overrides. */
export const card = css({
  bg: 'materialSurfaceRaised',
  borderColor: 'materialEdgeNormal',
  borderRadius: 'overlay',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '[0 25px 50px -12px {colors.materialDepthOverlay}]',
  left: '0',
  maxWidth: '[min(90vw, 24rem)]',
  p: '4',
  /** Parent PopupRoot uses pointer-events: none; content must opt back in. */
  pointerEvents: 'auto',
  position: 'absolute',
  top: '0',
  width: '[20rem]',
  zIndex: '100',
  '@supports (backdrop-filter: blur(1px))': {
    backdropFilter: '[blur(12px)]',
    bg: 'materialSurfaceGlass/80',
  },
});
