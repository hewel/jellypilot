import { css } from '@styled-system/css';

export const root = css({
  minWidth: '[0]',
});

export const section = css({
  display: 'grid',
  gap: '4',
});

export const toolbarTitle = css({
  color: 'onSurface',
  fontSize: '22',
  fontWeight: 'bold',
  lineHeight: '28',
});

export const toolbarCount = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  fontVariantNumeric: 'tabular-nums',
  lineHeight: '16',
});

export const grid = css({
  display: 'grid',
  gap: '3',
  gridTemplateColumns: '[repeat(auto-fill, minmax(min(100%, 160px), 1fr))]',
});

export const fade = css({
  animation: '[fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards]',
});

export const virtualGrid = css({
  animation: '[fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards]',
});

export const virtualCanvas = css({
  position: 'relative',
  width: 'full',
});

export const virtualRow = css({
  left: '0',
  position: 'absolute',
  top: '0',
  width: 'full',
});

export const loadMoreError = css({
  alignItems: 'center',
  display: 'flex',
  flexDirection: 'column',
  gap: '3',
  pt: '2',
});

export const error = css({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  textAlign: 'center',
});

export const pillButton = css({
  borderRadius: 'full',
});

export const icon4 = css({
  height: '4',
  width: '4',
});

export const spin = css({
  animation: '[spin 1s linear infinite]',
});

export const sentinel = css({
  height: '[1px]',
  width: 'full',
});

export const toolbarHeadingGroup = css({
  alignItems: 'baseline',
  columnGap: '3',
  display: 'flex',
  flexWrap: 'wrap',
  mr: 'auto',
  rowGap: '1',
});

export const toolbar = css({
  alignItems: 'center',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
  justifyContent: 'flex-end',
  mb: '6',
  py: '1',
  position: 'sticky',
  top: '0',
  zIndex: '40',
});

export const toolbarChrome = css({
  backdropFilter: '[blur(12px)]',
  bg: 'surface/85',
  borderColor: 'outlineVariant/50',
  borderRadius: '2xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  bottom: '[-6px]',
  boxShadow: 'lg',
  left: '[-16px]',
  opacity: '[0]',
  '&[data-pinned]': {
    opacity: '[1]',
  },
  pointerEvents: 'none',
  position: 'absolute',
  right: '[-16px]',
  top: '[-6px]',
  transitionDuration: '200',
  transitionProperty: '[opacity]',
  zIndex: '[-1]',
});

export const controlCapsule = css({
  alignItems: 'stretch',
  bg: 'surfaceContainerHigh/70',
  borderColor: 'outlineVariant',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  display: 'inline-flex',
  height: '10',
  p: '0_5',
  transitionDuration: '200',
  transitionProperty: '[border-color, background-color]',
  _hover: {
    borderColor: 'outline',
  },
  _groupDisabled: {
    opacity: '[0.5]',
  },
  sm: {
    height: '12',
  },
});

export const controlDivider = css({
  bg: 'outlineVariant',
  my: '1_5',
  width: '[1px]',
});

export const directionToggle = css({
  alignItems: 'center',
  bg: '[transparent]',
  border: '[0]',
  borderRadius: 'full',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'inline-flex',
  height: 'full',
  justifyContent: 'center',
  outline: 'none',
  transitionDuration: '200',
  transitionProperty: '[background-color, color]',
  width: '9',
  _hover: {
    bg: 'surfaceContainerHighest/70',
    color: 'onSurface',
  },
  '&[data-state="on"]': {
    bg: 'secondaryContainer',
    color: 'onSecondaryContainer',
  },
  sm: {
    width: '10',
  },
  _disabled: {
    cursor: 'not-allowed',
  },
});

export const sortTrigger = css({
  alignItems: 'center',
  bg: '[transparent]',
  border: '[0]',
  borderRadius: 'full',
  color: 'onSurface',
  cursor: 'pointer',
  display: 'inline-flex',
  gap: '2',
  height: 'full',
  outline: 'none',
  pl: '3',
  pr: '2_5',
  transitionDuration: '200',
  transitionProperty: '[background-color]',
  _hover: {
    bg: 'surfaceContainerHighest/70',
  },
  '&[data-state="open"]': {
    bg: 'surfaceContainerHighest/70',
  },
  _disabled: {
    cursor: 'not-allowed',
  },
});

export const sortTriggerIcon = css({
  color: 'secondary',
  flex: 'none',
});

export const sortTriggerText = css({
  display: 'grid',
  fontSize: '14',
  fontWeight: 'semibold',
  justifyItems: 'start',
  lineHeight: '20',
  maxWidth: '[10rem]',
});

export const sortSizer = css({
  gridArea: '1 / 1',
  height: '0',
  overflow: 'hidden',
  visibility: 'hidden',
  whiteSpace: 'pre',
});

export const sortValue = css({
  gridArea: '1 / 1',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const chevron = css({
  color: 'onSurfaceVariant',
  flex: 'none',
  transitionDuration: '200',
  transitionProperty: '[transform]',
});

export const chevronOpen = css({
  transform: 'rotate(180deg)',
});

export const statusTrigger = css({
  alignItems: 'center',
  bg: '[transparent]',
  border: '[0]',
  borderColor: 'outlineVariant',
  borderRadius: 'full',
  borderStyle: 'solid',
  borderWidth: '1px',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'inline-flex',
  gap: '2',
  height: '10',
  outline: 'none',
  px: '4',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, color]',
  _hover: {
    borderColor: 'outline',
    color: 'onSurface',
  },
  '&[data-state="open"]': {
    borderColor: 'outline',
    color: 'onSurface',
  },
  _disabled: {
    cursor: 'not-allowed',
  },
  sm: {
    height: '11',
  },
});

export const statusTriggerActive = css({
  bg: 'secondaryContainer/60',
  borderColor: 'secondary/50',
  color: 'onSecondaryContainer',
  _hover: {
    borderColor: 'secondary/70',
    color: 'onSecondaryContainer',
  },
  '&[data-state="open"]': {
    borderColor: 'secondary/70',
    color: 'onSecondaryContainer',
  },
});

export const statusTriggerIcon = css({
  flex: 'none',
});

export const statusTriggerText = css({
  fontSize: '14',
  fontWeight: 'semibold',
  lineHeight: '20',
});

export const statusBadge = css({
  alignItems: 'center',
  bg: 'secondary',
  borderRadius: 'full',
  color: 'onSecondary',
  display: 'inline-flex',
  fontSize: '11',
  fontWeight: 'bold',
  height: '4',
  justifyContent: 'center',
  lineHeight: '16',
  minWidth: '4',
  px: '1',
});

export const menuContent = css({
  animation: '[menuIn 0.18s {easings.emphasized}]',
  backdropFilter: '[blur(12px)]',
  bg: 'surfaceContainerLowest',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  borderStyle: 'solid',
  borderWidth: '1px',
  boxShadow: '2xl',
  maxHeight: '[15rem]',
  minWidth: '[12rem]',
  outline: 'none',
  overflowY: 'auto',
  p: '1_5',
  transformOrigin: 'top right',
  zIndex: '50',
});

export const menuLabel = css({
  color: 'onSurfaceVariant/80',
  fontSize: '11',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  px: '3',
  pb: '1_5',
  pt: '2',
  textTransform: 'uppercase',
});

export const menuItem = css({
  alignItems: 'center',
  borderRadius: 'lg',
  color: 'onSurfaceVariant',
  cursor: 'pointer',
  display: 'flex',
  fontSize: '14',
  justifyContent: 'space-between',
  lineHeight: '20',
  outline: 'none',
  px: '3',
  py: '2',
  transitionDuration: '150',
  transitionProperty: '[background-color, color]',
  _hover: {
    bg: 'surfaceContainerHigh',
    color: 'onSurface',
  },
  '&[data-highlighted]': {
    bg: 'surfaceContainerHigh',
    color: 'onSurface',
  },
  '&[data-state="checked"]': {
    color: 'onSurface',
  },
  '&[data-disabled]': {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
});

export const menuItemRow = css({
  alignItems: 'center',
  display: 'inline-flex',
  gap: '2',
});

export const menuItemIcon = css({
  color: 'secondary',
  flex: 'none',
});

export const menuText = css({
  fontWeight: 'medium',
});

export const menuCheck = css({
  color: 'secondary',
  height: '4',
  width: '4',
});

export const separator = css({
  borderTopColor: 'outlineVariant/60',
  borderTopStyle: 'solid',
  borderTopWidth: '1px',
  my: '1',
});
