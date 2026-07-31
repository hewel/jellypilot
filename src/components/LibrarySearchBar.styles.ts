import { css } from '@styled-system/css';

export const form = css({
  display: 'flex',
  width: 'full',
});

/* Compact 40px sidebar capsule: the control owns its glass material so the
 * sidebar host only supplies placement. */
export const control = css({
  alignItems: 'center',
  backdropFilter: '[blur(12px)]',
  bg: 'surface/85',
  borderColor: 'outlineVariant/70',
  borderRadius: 'xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'flex',
  flex: '1',
  height: '10',
  minWidth: '[0]',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, box-shadow]',
  _hover: {
    bg: 'surface/90',
    borderColor: 'outline',
  },
  _focusWithin: {
    borderColor: 'secondary',
    boxShadow: '[0 0 0 4px {colors.secondary/15}]',
    '& > svg': {
      color: 'secondary',
    },
  },
});

export const leadingIcon = css({
  color: 'onSurfaceVariant',
  flexShrink: '[0]',
  marginLeft: '3',
  marginRight: '2_5',
  pointerEvents: 'none',
  transitionDuration: '200',
  transitionProperty: '[color]',
});

export const input = css({
  appearance: 'none',
  bg: '[transparent !important]',
  borderWidth: '[0 !important]',
  boxShadow: '[none !important]',
  flex: '1',
  fontSize: '14',
  height: '[100% !important]',
  lineHeight: '20',
  minHeight: '[0 !important]',
  minWidth: '[0]',
  outline: '[none !important]',
  paddingInlineEnd: '1',
  paddingInlineStart: '0',
  '&::-webkit-search-cancel-button': {
    appearance: 'none',
    display: 'none',
  },
});

export const trailingAffordance = css({
  alignItems: 'center',
  display: 'flex',
  flexShrink: '[0]',
  height: 'full',
  justifyContent: 'center',
  paddingInline: '1',
  pointerEvents: 'none',
});

export const clearButton = css({
  height: '[2rem !important]',
  minHeight: '[0 !important]',
  minWidth: '[2rem !important]',
  pointerEvents: 'auto',
  width: '[2rem !important]',
});

export const keycap = css({
  alignItems: 'center',
  bg: 'surfaceContainerHigh/70',
  borderColor: 'outlineVariant/60',
  borderRadius: 'md',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant',
  display: 'inline-flex',
  fontSize: '11',
  fontWeight: 'semibold',
  height: '5',
  justifyContent: 'center',
  lineHeight: '[1]',
  minWidth: '5',
  pointerEvents: 'none',
});

/* Compact icon-only submit flush with the capsule end: full-height 40px hit
 * area with its own right-hand radii and an inset focus ring. */
export const submitButton = css({
  alignSelf: 'stretch',
  borderBottomLeftRadius: '[0 !important]',
  borderBottomRightRadius: '[{radii.xl} !important]',
  borderLeftColor: '[{colors.outlineVariant} !important]',
  borderLeftStyle: 'solid',
  borderLeftWidth: '1px',
  borderRightWidth: '[0 !important]',
  borderTopLeftRadius: '[0 !important]',
  borderTopRightRadius: '[{radii.xl} !important]',
  borderTopWidth: '[0 !important]',
  borderBottomWidth: '[0 !important]',
  boxShadow: '[none !important]',
  flexShrink: '[0]',
  height: '[100% !important]',
  minHeight: '[0 !important]',
  minWidth: '10',
  px: '0',
  _active: {
    transform: '[none !important]',
  },
  _focusVisible: {
    boxShadow: '[inset 0 0 0 2px {colors.secondary} !important]',
    outline: '[none !important]',
  },
});

/* Row-aligned collapsed trigger: one centered 40×40 Search action matching
 * the sidebar rail rows. */
export const collapsedTrigger = css({
  justifyContent: 'center',
});

export const collapsedTriggerIcon = css({
  height: '5',
  width: '5',
});
