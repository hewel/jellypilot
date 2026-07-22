import { css, cva } from '@styled-system/css';

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
  letterSpacing: '8',
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

export const checkboxLabel = css({
  cursor: 'pointer',
  userSelect: 'none',
});

export const log = cva({
  base: {
    overflowY: 'auto',
    borderRadius: '2xl',
    p: '3',
    boxShadow: 'inner',
    bg: 'surfaceContainerLowest/60',
    borderWidth: '1px',
    borderStyle: 'solid',
    borderColor: 'outlineVariant',
    display: 'grid',
    gap: '2_5',
  },
  variants: {
    size: {
      compact: {
        maxHeight: '[14rem]',
      },
      expanded: {
        maxHeight: '[24rem]',
      },
    },
  },
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
  letterSpacing: '5',
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
  color: 'secondary',
});

export const badgeWarn = css({
  bg: 'warningContainer/30',
  borderColor: 'warning/30',
  color: 'warning',
});

export const badgeError = css({
  bg: 'errorContainer/30',
  borderColor: 'error/30',
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

export const status = cva({
  base: {
    fontSize: '11',
    lineHeight: '16',
    fontWeight: 'bold',
    letterSpacing: '8',
    textTransform: 'uppercase',
  },
  variants: {
    tone: {
      copied: {
        color: 'tertiary',
      },
      failed: {
        color: 'error',
      },
    },
  },
});

export const actionButton = css({
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  color: 'onSurfaceVariant/90',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '8',
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
