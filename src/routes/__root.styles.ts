import { css } from '@styled-system/css';

/**
 * App scroll shell layout contract.
 * Avoid 100vw (includes scrollbar gutter) and fit-content min-width
 * (prevents shrinking) so stress widths never grow a horizontal scrollbar.
 */
export const appScrollViewportLayout = {
  height: '[100vh]',
  maxWidth: '[100%]',
  overflowX: 'auto',
  overflowY: 'auto',
  overscrollBehavior: 'contain',
  width: 'full',
} as const;

export const appScrollContentLayout = {
  maxWidth: '[100%]',
  minWidth: '[0]',
  width: 'full',
} as const;

export const viewport = css(appScrollViewportLayout);

export const content = css(appScrollContentLayout);
