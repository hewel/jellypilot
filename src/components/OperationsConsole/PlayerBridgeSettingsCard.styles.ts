import { css } from '@styled-system/css';

export const saveBadge = css({
  borderRadius: 'sm',
  px: '2_5',
  py: '0_5',
  fontSize: '11',
  fontWeight: 'bold',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: '[transparent]',
  letterSpacing: '5',
  textTransform: 'uppercase',
});

export const saveOk = css({
  bg: 'secondaryContainer/20',
  borderColor: 'secondary/20',
  color: 'secondary',
});

export const saveError = css({
  animation: '[pulse 1.8s {easings.inOut} infinite]',
  bg: 'errorContainer/20',
  borderColor: 'error/20',
  color: 'error',
});

export const field = css({
  display: 'block',
});

export const error = css({
  mt: '1_5',
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
});

export const helper = css({
  mt: '1_5',
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
});

export const detectRow = css({
  display: 'flex',
  flexDirection: 'column',
  gap: '2_5',
  sm: {
    flexDirection: 'row',
  },
});

export const flexInput = css({
  flex: '[1]',
  minWidth: '[0]',
});

export const chevron = css({
  color: 'onSurfaceVariant',
  height: 'lg',
  transitionDuration: '300',
  transitionProperty: '[color, transform]',
  width: 'lg',
});

export const chevronOpen = css({
  color: 'secondary',
  transform: '[rotate(180deg)]',
});

export const advancedPanel = css({
  mt: '3',
  borderRadius: '2xl',
  p: '4',
  bg: 'surfaceContainerLowest/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
});

export const subheading = css({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
});

export const subheadingAccent = css({
  bg: 'secondary',
  borderRadius: 'md',
  height: '3',
  width: '1',
});

export const textarea = css({
  color: 'onSurfaceVariant/80',
  fontFamily: 'mono',
  fontSize: '12',
  height: '[auto]',
  lineHeight: '16',
  py: '3_5',
  width: 'full',
});

export const languagePanel = css({
  position: 'relative',
  borderRadius: '2xl',
  p: '5',
  boxShadow: 'inner',
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
});

export const panelHeader = css({
  alignItems: 'flex-start',
  display: 'flex',
  flexWrap: 'wrap',
  gap: '4',
  justifyContent: 'space-between',
});

export const languageTitle = css({
  display: 'flex',
  alignItems: 'center',
  gap: '2',
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const languageIcon = css({
  color: 'secondary',
  height: 'lg',
  width: 'lg',
});

export const hidden = css({
  display: 'none',
});

export const languageGrid = css({
  display: 'grid',
  gap: '4',
  gridTemplateColumns: '[1fr]',
  mt: '5',
  sm: {
    gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
  },
});

export const customField = css({
  display: 'flex',
  minWidth: '[0]',
  flexDirection: 'column',
});

export const addRow = css({
  display: 'flex',
  gap: '2',
});

export const addButton = css({
  display: 'inline-flex',
  height: '14',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: '2xl',
  px: '4',
  color: 'onSecondaryContainer',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'bold',
  bg: 'secondaryContainer/40',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'secondary/20',
  cursor: 'pointer',
  minWidth: '[5.5rem]',
  transitionDuration: '200',
  transitionProperty: '[background-color, border-color, transform]',
  _hover: {
    bg: 'secondaryContainer/60',
    borderColor: 'secondary/40',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  _disabled: {
    opacity: '[0.5]',
    pointerEvents: 'none',
  },
});

export const plusIcon = css({
  height: '4',
  mr: '1',
  width: '4',
});

export const empty = css({
  mt: '5',
  borderRadius: '2xl',
  px: '4',
  py: '4',
  textAlign: 'center',
  fontSize: '12',
  lineHeight: '16',
  bg: 'surfaceContainerLowest/20',
  borderWidth: '1px',
  borderStyle: 'dashed',
  borderColor: 'outlineVariant',
  color: 'onSurfaceVariant/80',
});

export const list = css({
  position: 'relative',
  mt: '5',
  display: 'flex',
  flexDirection: 'column',
  gap: '2',
  zIndex: '10',
});

export const item = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: '3',
  borderRadius: 'xl',
  px: '4',
  py: '2_5',
  bg: 'surfaceContainerLowest/60',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  transitionProperty: '[background-color]',
  _hover: {
    bg: 'surfaceContainerLowest/80',
  },
});

export const itemPreview = css({
  display: 'flex',
  minWidth: '[0]',
  flexGrow: '[1]',
  alignItems: 'center',
  gap: '3',
});

export const indexBadge = css({
  display: 'flex',
  height: '6',
  width: '6',
  flexShrink: '[0]',
  alignItems: 'center',
  justifyContent: 'center',
  borderRadius: 'lg',
  color: 'secondary',
  fontSize: '11',
  fontWeight: 'bold',
  boxShadow: 'inner',
  bg: 'surfaceContainerHigh/60',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  fontFamily: 'mono',
  fontVariantNumeric: 'tabular-nums',
});

export const code = css({
  color: 'onSurface',
  flexShrink: '[0]',
  fontFamily: 'mono',
  fontSize: '14',
  fontWeight: 'bold',
});

export const itemLabel = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const itemActions = css({
  display: 'flex',
  flexShrink: '[0]',
  alignItems: 'center',
  gap: '1_5',
});

export const detectButton = css({
  minHeight: '14',
  sm: {
    minHeight: '[0]',
  },
});

export const advancedTrigger = css({
  fontWeight: 'bold',
  px: '0',
  _hover: {
    color: 'secondary',
  },
});

export const clearButton = css({
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  fontSize: '13',
  fontWeight: 'bold',
  minWidth: '[0]',
  py: '1',
  px: '3',
  _hover: {
    bg: 'secondary/5',
    borderColor: 'secondary',
  },
});

export const smallIconButton = css({
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
  borderRadius: 'lg',
  height: '8',
  width: '8',
  _hover: {
    borderColor: 'secondary',
    color: 'secondary',
  },
});

export const deleteButton = css({
  _hover: {
    borderColor: 'error',
    color: 'error',
  },
});

export const fullWidth = css({
  width: 'full',
});

export const mono = css({
  fontFamily: 'mono',
});

export const icon4 = css({
  height: '4',
  width: '4',
});
