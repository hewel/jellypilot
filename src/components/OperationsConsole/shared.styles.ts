import { css } from '@styled-system/css';

export const sectionIcon = {
  primary: css({
    color: 'primary',
    filter: '[drop-shadow(0 0 8px {colors.primary/40})]',
    height: '5',
    width: '5',
  }),
  secondary: css({
    color: 'secondary',
    filter: '[drop-shadow(0 0 8px {colors.secondary/40})]',
    height: '5',
    width: '5',
  }),
  plain: css({
    height: '6',
    width: '6',
  }),
} as const;

export const grid3 = css({
  display: 'grid',
  gap: '4',
  gridTemplateColumns: '[1fr]',
  md: {
    gridTemplateColumns: '[repeat(3, minmax(0, 1fr))]',
  },
});

export const tile = css({
  position: 'relative',
  overflow: 'hidden',
  borderRadius: '2xl',
  p: '4',
  backdropFilter: '[blur(4px)]',
  bg: 'surfaceContainerHigh/30',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant/60',
});

export const span2 = css({
  md: {
    gridColumn: '[span 2 / span 2]',
  },
});

export const tileWatermark = css({
  opacity: '[0.05]',
  p: '3',
  position: 'absolute',
  right: '0',
  top: '0',
});

export const watermarkIcon = css({
  width: '12',
  height: '12',
});

export const overline = css({
  color: 'onSurfaceVariant',
  fontSize: '11',
  lineHeight: '16',
  fontWeight: 'bold',
  letterSpacing: '[0.08em]',
  textTransform: 'uppercase',
});

export const value = css({
  mt: '1_5',
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'bold',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const monoValue = css({
  mt: '1_5',
  color: 'secondary',
  fontFamily: 'mono',
  fontSize: '14',
  lineHeight: '20',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const bodyText = css({
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
});

export const warning = css({
  mt: '2',
  display: 'flex',
  alignItems: 'flex-start',
  gap: '2',
  color: 'warning',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
});

export const warningIcon = css({
  flexShrink: '[0]',
  height: '3_5',
  mt: '0_5',
  width: '3_5',
});

export const actionRow = css({
  mt: '6',
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '3',
});

export const stack4 = css({
  display: 'grid',
  gap: '4',
});

export const fieldset = css({
  border: 0,
  display: 'grid',
  gap: '3',
  gridTemplateColumns: '[1fr]',
  margin: '0',
  padding: '0',
});

export const choice = css({
  borderRadius: '2xl',
  px: '4',
  py: '3',
  textAlign: 'left',
  backdropFilter: '[blur(4px)]',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  cursor: 'pointer',
  transitionDuration: '300',
  transitionProperty: '[background-color, border-color, box-shadow, transform]',
  _active: {
    transform: '[scale(0.96)]',
  },
  _hover: {
    bg: 'surfaceContainerHigh/60',
    borderColor: 'primary/50',
  },
});

export const choiceIdle = css({
  bg: 'surfaceContainerHigh/40',
  color: 'onSurface',
});

export const choiceSelected = css({
  bg: 'primaryContainer/35',
  borderColor: 'primary',
  boxShadow: '[0 0 15px {colors.primary/25}]',
  color: 'onPrimaryContainer',
  fontWeight: 'semibold',
});

export const choiceTitle = css({
  display: 'block',
  color: 'onSurface',
  fontSize: '16',
  lineHeight: '24',
  fontWeight: 'semibold',
});

export const choiceDescription = css({
  display: 'block',
  mt: '1',
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
  opacity: '[0.8]',
});

export const saving = css({
  display: 'flex',
  alignItems: 'center',
  gap: '1_5',
  color: 'secondary',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
  animation: '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
});

export const pingDot = css({
  animation: '[ping 1s cubic-bezier(0, 0, 0.2, 1) infinite]',
  bg: 'secondary',
  borderRadius: 'full',
  boxShadow: '[0 0 8px {colors.secondary}]',
  height: '1_5',
  width: '1_5',
});

export const errorPanel = css({
  borderRadius: '2xl',
  px: '4',
  py: '3',
  color: 'onErrorContainer',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
  bg: 'errorContainer/20',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'error/30',
});

export const mutedOutlinedButton = css({
  color: 'onSurfaceVariant',
  _hover: {
    borderColor: 'primary/50',
    color: 'onSurface',
  },
});

export const refreshButton = css({
  bg: 'surfaceContainerHigh/20',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
  borderRadius: 'xl',
  marginLeft: 'auto',
  _hover: {
    borderColor: 'secondary',
    color: 'secondary',
  },
});

export const icon4_5 = css({
  height: '[1.125rem]',
  width: '[1.125rem]',
});
