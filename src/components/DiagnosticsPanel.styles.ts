import { css } from '@styled-system/css';

export const root = css({
  display: 'grid',
  gap: '4',
});

export const header = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: '3',
  px: '1',
});

export const count = css({
  color: 'onSurfaceVariant/80',
  fontFamily: 'mono',
  fontSize: '11',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'semibold',
});

export const checkboxRoot = css({
  display: 'inline-flex',
  alignItems: 'center',
  gap: '2_5',
  color: 'onSurfaceVariant/95',
  cursor: 'pointer',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  textTransform: 'uppercase',
  transitionProperty: '[opacity]',
  userSelect: 'none',
  verticalAlign: 'top',
  _disabled: {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
});

export const checkbox = css({
  display: 'inline-flex',
  alignItems: 'center',
  justifyContent: 'center',
  flexShrink: '[0]',
  color: 'onPrimary',
  borderRadius: 'lg',
  fontSize: '11',
  lineHeight: 'none',
  bg: 'surfaceContainerHigh',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outline',
  height: '[1.375rem]',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, box-shadow]',
  width: '[1.375rem]',
  _hover: {
    borderColor: 'primary/60',
  },
  '&[data-state="checked"], &[data-state="indeterminate"]': {
    backgroundImage: '[linear-gradient(135deg, {colors.primary}, {colors.primaryGradientEnd})]',
    borderColor: 'primary',
  },
  '&[data-focus-visible]': {
    boxShadow: '[0 0 0 2px {colors.primary/50}]',
    outline: 'none',
  },
});

export const indicator = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontWeight: 'black',
});

export const checkboxLabel = css({
  cursor: 'pointer',
  userSelect: 'none',
});

export const log = css({
  overflowY: 'auto',
  borderRadius: '2xl',
  p: '3',
  boxShadow: 'inner',
  backdropFilter: '[blur(4px)]',
  bg: 'surfaceContainerLowest/60',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  display: 'grid',
  gap: '2_5',
});

export const compactLog = css({
  maxHeight: '[14rem]',
});

export const expandedLog = css({
  maxHeight: '[24rem]',
});

export const empty = css({
  color: 'onSurfaceVariant/60',
  fontFamily: 'mono',
  fontSize: '12',
  lineHeight: '16',
  py: '10',
  textAlign: 'center',
});

export const entry = css({
  position: 'relative',
  overflow: 'hidden',
  borderRadius: 'xl',
  px: '3_5',
  py: '2',
  color: 'onSurfaceVariant',
  fontFamily: 'mono',
  fontSize: '12',
  lineHeight: '20',
  bg: 'surfaceContainerLowest/70',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/40',
  transitionProperty: '[background-color, border-color]',
  _hover: {
    bg: 'surfaceContainerLowest/90',
    borderColor: 'outlineVariant/60',
  },
});

export const entryInner = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  columnGap: '3',
  rowGap: '1_5',
  position: 'relative',
  zIndex: '10',
});

export const time = css({
  color: 'outline',
  fontWeight: 'semibold',
  userSelect: 'none',
});

export const badge = css({
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: '[currentColor]',
  borderRadius: 'md',
  fontSize: '10',
  fontWeight: 'bold',
  letterSpacing: '[0.05em]',
  px: '2',
  py: '0_5',
  userSelect: 'none',
});

export const badgeTrace = css({
  bg: 'surfaceContainerHighest',
  borderColor: 'outlineVariant/40',
  color: 'outline',
});

export const badgeDebug = css({
  bg: 'surfaceContainerHighest',
  borderColor: 'outline/30',
  color: 'onSurfaceVariant',
});

export const badgeInfo = css({
  bg: 'secondaryContainer/30',
  borderColor: 'secondary/30',
  boxShadow: '[0 0 6px {colors.secondary/10}]',
  color: 'secondary',
});

export const badgeWarn = css({
  bg: 'warningContainer/30',
  borderColor: 'warning/30',
  boxShadow: '[0 0 6px {colors.warning/10}]',
  color: 'warning',
});

export const badgeError = css({
  bg: 'errorContainer/30',
  borderColor: 'error/30',
  boxShadow: '[0 0 6px {colors.error/10}]',
  color: 'error',
});

export const message = css({
  color: 'onSurfaceVariant',
  fontWeight: 'medium',
  overflowWrap: 'anywhere',
});

export const actions = css({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  justifyContent: 'flex-end',
  gap: '3',
  px: '1',
});

export const status = css({
  fontSize: '11',
  lineHeight: '16',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  textTransform: 'uppercase',
});

export const statusCopied = css({
  color: 'tertiary',
  filter: '[drop-shadow(0 0 6px {colors.tertiary/20})]',
});

export const statusFailed = css({
  color: 'error',
});

export const actionButton = css({
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  color: 'onSurfaceVariant/90',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  lineHeight: '16',
  textTransform: 'uppercase',
  _hover: {
    bg: 'secondary/5',
    borderColor: 'secondary',
  },
});

export const dangerActionButton = css({
  _hover: {
    bg: 'error/5',
    borderColor: 'error',
    color: 'error',
  },
});
