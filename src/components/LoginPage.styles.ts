import { css, cx } from '@styled-system/css';

export const shell = css({
  position: 'relative',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  overflowY: 'auto',
  py: '10',
});

export const footer = css({
  mt: '8',
});

export const card = css({
  position: 'relative',
  mx: 'auto',
  overflow: 'hidden',
  boxShadow: '2xl',
});

export const accent = css({
  backgroundImage:
    '[linear-gradient(90deg, transparent, color-mix(in srgb, {colors.primary} 55%, transparent), transparent)]',
  height: '0_5',
  left: '0',
  position: 'absolute',
  top: '0',
  width: 'full',
});

export const stack7 = css({
  display: 'grid',
  gap: '7',
});

export const sectionTitle = css({
  display: 'flex',
  alignItems: 'center',
  gap: '2_5',
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '24',
  lineHeight: '32',
  fontWeight: 'bold',
});

export const titleBar = css({
  bg: 'primary',
  borderRadius: 'md',
  height: '5',
  width: '1_5',
});

export const description = css({
  mt: '1_5',
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
});

export const serverGrid = css({
  display: 'grid',
  gap: '3',
  gridTemplateColumns: '[1fr]',
  sm: {
    gridTemplateColumns: '[auto minmax(0, 1fr)]',
  },
});

export const segmented = css({
  display: 'grid',
  borderRadius: '2xl',
  p: '1',
  bg: 'surfaceContainerHigh/40',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
});

export const segmented2 = css({
  gridTemplateColumns: '[repeat(2, minmax(0, 1fr))]',
});

export const segmented1 = css({
  gridTemplateColumns: '[1fr]',
});

export const segment = css({
  borderRadius: 'xl',
  px: '4',
  py: '3',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
  bg: '[transparent]',
  border: 0,
  cursor: 'pointer',
  letterSpacing: '[0]',
  textTransform: 'uppercase',
  transitionDuration: '300',
  transitionProperty: '[background-color, color, box-shadow, transform]',
  _hover: {
    bg: 'surfaceContainerHighest/40',
    color: 'onSurface',
  },
  _active: {
    transform: '[scale(0.96)]',
  },
  _disabled: {
    cursor: 'not-allowed',
    opacity: '[0.5]',
  },
});

export const segmentIdle = css({
  color: 'onSurfaceVariant',
});

export const segmentSelected = css({
  backgroundImage: '[linear-gradient(90deg, {colors.primary}, {colors.primaryGradientEnd})]',
  boxShadow: '[0 4px 8px {colors.primary/25}]',
  color: 'onPrimary',
  fontWeight: 'bold',
});

export const srOnly = css({
  border: 0,
  clip: '[rect(0 0 0 0)]',
  height: 'px',
  margin: '[-1px]',
  overflow: 'hidden',
  padding: '0',
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: 'px',
});

export const preview = css({
  position: 'relative',
  overflow: 'hidden',
  borderRadius: '2xl',
  p: '4',
  backdropFilter: '[blur(4px)]',
  bg: 'surfaceContainerLowest/40',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
});

export const previewStripe = css({
  bg: 'secondary',
  bottom: '0',
  left: '0',
  position: 'absolute',
  top: '0',
  width: '[3px]',
});

export const overline = css({
  fontSize: '11',
  lineHeight: '16',
  fontWeight: 'bold',
  color: 'onSurfaceVariant/90',
  letterSpacing: '[0.08em]',
  textTransform: 'uppercase',
});

export const previewValue = css({
  mt: '1',
  color: 'onSurfaceVariant',
  fontFamily: 'mono',
  fontSize: '14',
  lineHeight: '20',
  overflowWrap: 'anywhere',
});

export const previewReady = css({
  color: 'secondary',
  filter: '[drop-shadow(0 0 8px {colors.secondary/15})]',
  fontWeight: 'semibold',
});

export const previewEmpty = css({
  color: 'warning',
});

export const fieldBlock = css({
  display: 'block',
});

export const label = css({
  display: 'block',
  mb: '1_5',
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'bold',
  letterSpacing: '[0.05em]',
  textTransform: 'uppercase',
});

export const tabsList = css({
  mb: '6',
});

export const quickPanel = css({
  position: 'relative',
  overflow: 'hidden',
  borderRadius: '3xl',
  p: '6',
  textAlign: 'center',
  backdropFilter: '[blur(4px)]',
  bg: 'secondaryContainer/20',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'secondary/25',
  transitionProperty: '[color]',
});

export const quickPanelGlow = css({
  backgroundImage: '[linear-gradient(to bottom, {colors.secondary/5}, transparent)]',
  inset: '0',
  pointerEvents: 'none',
  position: 'absolute',
});

export const radar = css({
  position: 'relative',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  mx: 'auto',
  mb: '4',
  width: '20',
  height: '20',
  borderRadius: 'full',
  bg: 'secondary/5',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'secondary/20',
});

export const radarRing = css({
  animation: '[radar-pulse 2.2s cubic-bezier(0.2, 0.8, 0.2, 1) infinite]',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'secondary/40',
  borderRadius: 'full',
  inset: '0',
  position: 'absolute',
});

export const radarRing2 = css({
  animationDelay: '[0.7s]',
  borderColor: 'secondary/30',
});

export const radarRing3 = css({
  animationDelay: '[1.4s]',
  borderColor: 'secondary/20',
});

export const towerIcon = css({
  color: 'secondary',
  filter: '[drop-shadow(0 0 8px {colors.secondary/40})]',
  height: '9',
  width: '9',
});

export const pulse = css({
  animation: '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
});

export const quickText = css({
  color: 'onSecondaryContainer',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'medium',
});

export const quickHint = css({
  mt: '2',
  color: 'onSurfaceVariant/80',
  fontSize: '12',
  lineHeight: '16',
});

export const codeBox = css({
  display: 'inline-flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  mt: '6',
  borderRadius: '2xl',
  px: '6',
  py: '3_5',
  boxShadow: 'inner',
  bg: 'surfaceContainerLowest/80',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'outlineVariant',
});

export const codeLabel = css({
  mb: '1',
  color: 'onSurfaceVariant/80',
  fontSize: '10',
  fontWeight: 'bold',
  letterSpacing: '[0.2em]',
  textTransform: 'uppercase',
});

export const code = css({
  color: 'secondary',
  filter: '[drop-shadow(0 0 10px {colors.secondary/55})]',
  fontFamily: 'mono',
  fontSize: '36',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'bold',
  letterSpacing: '[0.25em]',
  lineHeight: '44',
  paddingLeft: '[0.25em]',
});

export const waiting = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: '2',
  mt: '5',
  color: 'secondary',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'bold',
  animation: '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
  letterSpacing: '[0.05em]',
  textTransform: 'uppercase',
});

export const waitingDot = css({
  bg: 'secondary',
  borderRadius: 'full',
  boxShadow: '[0 0 8px {colors.secondary}]',
  height: '2',
  width: '2',
});

export const stack4 = css({
  display: 'grid',
  gap: '4',
});

export const remember = css({
  display: 'inline-flex',
  alignItems: 'center',
  gap: '2_5',
  mt: '2_5',
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '20',
  cursor: 'pointer',
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
  fontSize: '11',
  lineHeight: 'none',
  borderRadius: 'lg',
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

export const checkboxIndicator = css({
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontWeight: 'black',
});

export const checkboxLabel = css({
  cursor: 'pointer',
  fontWeight: 'medium',
  transitionProperty: '[color]',
  userSelect: 'none',
  _hover: {
    color: 'onSurfaceVariant',
  },
});

export const alert = css({
  display: 'flex',
  alignItems: 'flex-start',
  gap: '3',
  borderRadius: '2xl',
  p: '4',
  color: 'onErrorContainer',
  bg: 'errorContainer/20',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'error/30',
});

export const alertIcon = css({
  mt: '0_5',
  width: '5',
  height: '5',
  flexShrink: '[0]',
  color: 'error',
});

export const alertTitle = css({
  color: 'error',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'bold',
});

export const alertMessage = css({
  mt: '0_5',
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
});

export const main = css({
  maxWidth: '[48rem]',
  position: 'relative',
  width: 'full',
  zIndex: '10',
});

export const hero = css({
  position: 'relative',
  mb: '8',
  textAlign: 'center',
});

export const heroIconWrap = css({
  position: 'relative',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  mx: 'auto',
  mb: '6',
  width: '24',
  height: '24',
  borderRadius: 'full',
  bg: 'primary/5',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'primary/20',
  boxShadow: '[0 0 30px {colors.primary/20}]',
});

export const heroRing = css({
  animation: '[ping 1s cubic-bezier(0, 0, 0.2, 1) infinite]',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'primary/30',
  borderRadius: 'full',
  inset: '0',
  opacity: '[0.25]',
  position: 'absolute',
});

export const heroPulseRing = css({
  animation: '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'secondary/25',
  borderRadius: 'full',
  inset: '2',
  position: 'absolute',
});

export const heroOrbit = css({
  animation: '[spin 60s linear infinite]',
  borderWidth: '1px',
  borderStyle: 'dashed',
  borderColor: 'primary/10',
  borderRadius: 'full',
  inset: '4',
  position: 'absolute',
});

export const heroIcon = css({
  color: 'primary',
  filter: '[drop-shadow(0 0 12px {colors.primary/55})]',
  height: '10',
  width: '10',
});

export const badge = css({
  display: 'inline-flex',
  alignItems: 'center',
  gap: '2_5',
  mb: '3_5',
  borderRadius: 'full',
  px: '3_5',
  py: '1',
  bg: 'secondary/5',
  borderWidth: '1px',
  borderStyle: 'solid',
  borderColor: 'secondary/20',
});

export const badgeDot = css({
  animation: '[pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite]',
  bg: 'secondary',
  borderRadius: 'full',
  boxShadow: '[0 0 8px {colors.secondary}]',
  height: '1_5',
  width: '1_5',
});

export const badgeText = css({
  color: 'secondary',
  fontSize: '10',
  fontWeight: 'bold',
  letterSpacing: '[0.18em]',
  textTransform: 'uppercase',
});

export const appTitle = css({
  color: 'onSurface',
  fontFamily: 'display',
  fontSize: '45',
  fontWeight: 'bold',
  letterSpacing: '[0]',
  lineHeight: '52',
});

export const appDescription = css({
  mx: 'auto',
  mt: '2',
  color: 'onSurfaceVariant',
  fontSize: '16',
  lineHeight: '24',
  maxWidth: '[28rem]',
});

export const fullWidth = css({
  width: 'full',
});

export const icon3_5 = css({
  height: '3_5',
  width: '3_5',
});

export const icon5 = css({
  height: '5',
  width: '5',
});

export const spinner = css({
  animation: '[spin 1s linear infinite]',
});

export { cx };
