import { css } from '@styled-system/css';

export const form = css({
  display: 'flex',
  maxWidth: '[32rem]',
  width: 'full',
});

/* The control owns its own glass capsule (same material as the browse
 * toolbar chrome) so every host can stay quiet: Home only adds a top-fade
 * scrim, the browse toolbar only adds its pinned backdrop. */
export const control = css({
  alignItems: 'center',
  backdropFilter: '[blur(12px)]',
  bg: 'surface/85',
  borderColor: 'outlineVariant/70',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: 'lg',
  display: 'flex',
  flex: '1',
  height: '11',
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
  sm: {
    height: '12',
  },
});

export const leadingIcon = css({
  color: 'onSurfaceVariant',
  flexShrink: '[0]',
  hideBelow: 'sm',
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
  paddingInlineEnd: '1',
  paddingInlineStart: '3',
  sm: {
    paddingInlineEnd: '0',
    paddingInlineStart: '0',
  },
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
  paddingInline: '2',
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
  hideBelow: 'sm',
  justifyContent: 'center',
  lineHeight: '[1]',
  minWidth: '5',
  pointerEvents: 'none',
});

/* Flush end-cap: the control no longer clips overflow, so the cap carries its
 * own right-hand radii and an inset focus ring that never gets cut off. */
export const submitButton = css({
  alignSelf: 'stretch',
  borderBottomLeftRadius: '[0 !important]',
  borderBottomRightRadius: '[{radii.2xl} !important]',
  borderLeftColor: '[{colors.outlineVariant} !important]',
  borderLeftStyle: 'solid',
  borderLeftWidth: '1px',
  borderRightWidth: '[0 !important]',
  borderTopLeftRadius: '[0 !important]',
  borderTopRightRadius: '[{radii.2xl} !important]',
  borderTopWidth: '[0 !important]',
  borderBottomWidth: '[0 !important]',
  boxShadow: '[none !important]',
  flexShrink: '[0]',
  height: '[100% !important]',
  minHeight: '[0 !important]',
  minWidth: '11',
  px: '3',
  _active: {
    transform: '[none !important]',
  },
  _focusVisible: {
    boxShadow: '[inset 0 0 0 2px {colors.secondary} !important]',
    outline: '[none !important]',
  },
  sm: {
    minWidth: '[5.25rem]',
    px: '4',
  },
});

export const submitLabel = css({
  hideBelow: 'sm',
});

export const submitIcon = css({
  hideFrom: 'sm',
});
