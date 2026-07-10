import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

const colors = {
  background: 'var(--jellypilot-color-surface-container)',
  backgroundRaised: 'var(--jellypilot-color-surface-container-low)',
  border: 'var(--jellypilot-color-outline-variant)',
  borderStrong: 'var(--jellypilot-color-outline)',
  onSurface: 'var(--jellypilot-color-on-surface)',
  onSurfaceVariant: 'var(--jellypilot-color-on-surface-variant)',
  primary: 'var(--jellypilot-color-primary)',
  secondary: 'var(--jellypilot-color-secondary)',
  secondaryContainer: 'var(--jellypilot-color-secondary-container)',
  error: 'var(--jellypilot-color-error)',
  errorContainer: 'var(--jellypilot-color-error-container)',
};

export const card = style([
  atomic({
    mx: 'auto',
    position: 'relative',
    overflow: 'hidden',
    rounded: '3xl',
  }),
  {
    boxShadow: 'var(--jellypilot-shadow-2xl)',
    background: colors.background,
    border: `1px solid ${colors.border}`,
  },
]);

export const accent = style({
  background: colors.primary,
  height: '0.125rem',
  left: 0,
  position: 'absolute',
  top: 0,
  width: '100%',
});

export const stack7 = atomic({
  display: 'grid',
  gap: 7,
});

export const sectionTitle = style([
  atomic({
    display: 'flex',
    alignItems: 'center',
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-24)',
    lineHeight: 32,
    fontWeight: 'bold',
  }),
  {
    fontFamily: 'var(--jellypilot-font-display)',
  },
]);

export const titleBar = style({
  background: colors.primary,
  borderRadius: 'var(--jellypilot-radius-md)',
  height: 'var(--jellypilot-space-5)',
  width: 'var(--jellypilot-space-1_5)',
});

export const description = atomic({
  marginTop: 1.5,
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 20,
});

export const serverGrid = style([
  atomic({
    display: 'grid',
    gap: 3,
    gridTemplateColumns: '1fr',
  }),
  {
    '@media': {
      'screen and (min-width: 640px)': {
        gridTemplateColumns: 'auto minmax(0, 1fr)',
      },
    },
  },
]);

export const segmented = style([
  atomic({
    display: 'grid',
    rounded: '2xl',
    p: 1,
  }),
  {
    background: colors.backgroundRaised,
    border: `1px solid ${colors.border}`,
  },
]);

export const segmented2 = style({
  gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
});

export const segment = style([
  atomic({
    borderRadius: 'xl',
    px: 4,
    py: 3,
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 20,
    fontWeight: 'semibold',
  }),
  {
    background: 'transparent',
    border: 0,
    cursor: 'pointer',
    letterSpacing: 0,
    textTransform: 'uppercase',
    selectors: {
      '&:disabled': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const segmentIdle = style({
  color: colors.onSurfaceVariant,
});

export const segmentSelected = style({
  background: colors.primary,
  color: 'var(--jellypilot-color-on-primary)',
});

export const srOnly = style({
  border: 0,
  clip: 'rect(0 0 0 0)',
  height: '1px',
  margin: '-1px',
  overflow: 'hidden',
  padding: 0,
  position: 'absolute',
  whiteSpace: 'nowrap',
  width: '1px',
});

export const preview = style([
  atomic({
    position: 'relative',
    overflow: 'hidden',
    borderRadius: '2xl',
    p: 4,
  }),
  {
    background: colors.backgroundRaised,
    border: `1px solid ${colors.border}`,
  },
]);

export const previewStripe = style({
  background: colors.secondary,
  bottom: 0,
  left: 0,
  position: 'absolute',
  top: 0,
  width: '0.1875rem',
});

export const overline = style([
  atomic({
    fontSize: 'var(--jellypilot-font-size-11)',
    lineHeight: 16,
    fontWeight: 'bold',
  }),
  {
    color: colors.onSurfaceVariant,
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const previewValue = style([
  atomic({
    marginTop: 1,
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 20,
  }),
  {
    fontFamily: 'var(--jellypilot-font-mono)',
    overflowWrap: 'anywhere',
  },
]);

export const previewReady = style({
  color: colors.secondary,
  fontWeight: 'var(--jellypilot-font-weight-semibold)',
});

export const previewEmpty = style({
  color: 'var(--jellypilot-color-warning)',
});

export const label = style([
  atomic({
    display: 'block',
    marginBottom: 1.5,
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 16,
    fontWeight: 'bold',
  }),
]);

export const quickPanel = style([
  atomic({
    position: 'relative',
    borderRadius: '3xl',
    p: 6,
    textAlign: 'center',
    marginTop: 2,
    display: 'grid',
    gap: 3,
  }),
  {
    background: colors.secondaryContainer,
    border: `1px solid ${colors.border}`,
  },
]);

export const quickPanelGlow = style({
  display: 'none',
});

export const radar = style([
  atomic({
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    mx: 'auto',
    mb: 4,
    width: 20,
    height: 20,
    borderRadius: 'full',
  }),
  {
    border: `1px solid ${colors.borderStrong}`,
  },
]);

export const radarRing = style({
  position: 'absolute',
  inset: 0,
  borderRadius: '50%',
  border: `1px solid ${colors.borderStrong}`,
});

export const radarRing2 = style({
  inset: '0.25rem',
  opacity: 0.65,
});

export const radarRing3 = style({
  inset: '0.5rem',
  opacity: 0.4,
});

export const towerIcon = style({
  color: colors.secondary,
  height: 'var(--jellypilot-space-9)',
  width: 'var(--jellypilot-space-9)',
});

export const pulse = style({
  opacity: 0.85,
});

export const quickText = atomic({
  color: 'var(--jellypilot-color-on-secondary-container)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 20,
  fontWeight: 'medium',
});

export const quickHint = style([
  atomic({
    marginTop: 2,
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 16,
  }),
  {
    color: colors.onSurfaceVariant,
  },
]);

export const codeBox = style([
  atomic({
    display: 'inline-flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    marginTop: 6,
    borderRadius: '2xl',
    px: 6,
    py: 3.5,
    lineHeight: 'none',
  }),
  {
    border: `1px solid ${colors.border}`,
    background: colors.background,
  },
]);

export const codeLabel = style([
  atomic({
    marginBottom: 1,
    fontSize: 'var(--jellypilot-font-size-10)',
    lineHeight: 1.2,
    fontWeight: 'bold',
  }),
  {
    color: colors.onSurfaceVariant,
    letterSpacing: '0.2em',
    textTransform: 'uppercase',
  },
]);

export const code = style({
  color: colors.secondary,
  fontFamily: 'var(--jellypilot-font-mono)',
  fontSize: 'var(--jellypilot-font-size-36)',
  fontVariantNumeric: 'tabular-nums',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  letterSpacing: '0.25em',
  lineHeight: 'var(--jellypilot-line-height-44)',
  paddingLeft: '0.25rem',
});

export const waiting = style([
  atomic({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
    marginTop: 5,
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-12)',
    lineHeight: 16,
    fontWeight: 'bold',
  }),
  {
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
  },
]);

export const waitingDot = style({
  background: colors.secondary,
  borderRadius: 'var(--jellypilot-radius-full)',
  height: 'var(--jellypilot-space-2)',
  width: 'var(--jellypilot-space-2)',
});

export const stack4 = atomic({
  display: 'grid',
  gap: 4,
});

export const remember = style([
  atomic({
    display: 'inline-flex',
    alignItems: 'center',
    gap: 2.5,
    marginTop: 2.5,
    color: 'var(--jellypilot-color-on-surface)',
    fontSize: 'var(--jellypilot-font-size-14)',
    lineHeight: 20,
  }),
  {
    userSelect: 'none',
  },
]);

export const textInput = atomic({
  width: 'full',
});

export const fullWidth = atomic({
  width: 'full',
});

export const icon5 = style({
  height: 'var(--jellypilot-space-5)',
  width: 'var(--jellypilot-space-5)',
});

export const spinner = style({
  animation: 'spin 1s linear infinite',
});

export const alert = style([
  atomic({
    display: 'flex',
    alignItems: 'flex-start',
    gap: 3,
    borderRadius: '2xl',
    p: 4,
    color: 'var(--jellypilot-color-on-error)',
  }),
  {
    background: colors.errorContainer,
    border: `1px solid ${colors.error}`,
  },
]);

export const alertIcon = atomic({
  marginTop: 0.5,
  width: 5,
  height: 5,
  flexShrink: 0,
  color: 'var(--jellypilot-color-error)',
});

export const alertTitle = atomic({
  color: 'var(--jellypilot-color-error)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 20,
  fontWeight: 'bold',
});

export const alertMessage = atomic({
  marginTop: 0.5,
  color: 'var(--jellypilot-color-on-surface-variant)',
  fontSize: 'var(--jellypilot-font-size-14)',
  lineHeight: 20,
});

export const shell = atomic({
  position: 'relative',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  overflowY: 'auto',
  paddingY: 10,
});

export const main = style({
  maxWidth: '48rem',
  position: 'relative',
  width: '100%',
  zIndex: 10,
});

export const hero = style([
  atomic({
    position: 'relative',
    marginBottom: 8,
    textAlign: 'center',
  }),
]);

export const heroIconWrap = style([
  atomic({
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    marginX: 'auto',
    marginBottom: 6,
    width: 24,
    height: 24,
    borderRadius: 'full',
  }),
  {
    border: `1px solid ${colors.borderStrong}`,
    background: colors.background,
  },
]);

export const heroRing = style({
  border: `1px solid ${colors.border}`,
  borderRadius: 'var(--jellypilot-radius-full)',
  inset: 0,
  position: 'absolute',
});

export const heroPulseRing = style({
  border: `1px solid ${colors.border}`,
  borderRadius: 'var(--jellypilot-radius-full)',
  inset: 'var(--jellypilot-space-2)',
  position: 'absolute',
});

export const heroOrbit = style({
  border: `1px dashed ${colors.border}`,
  borderRadius: 'var(--jellypilot-radius-full)',
  inset: 'var(--jellypilot-space-4)',
  position: 'absolute',
});

export const heroIcon = style({
  color: colors.primary,
  height: 'var(--jellypilot-space-10)',
  width: 'var(--jellypilot-space-10)',
});

export const badge = style([
  atomic({
    display: 'inline-flex',
    alignItems: 'center',
    gap: 2.5,
    marginBottom: 3.5,
    borderRadius: 'full',
    px: 3.5,
    py: 1,
  }),
  {
    background: colors.secondaryContainer,
    border: `1px solid ${colors.border}`,
  },
]);

export const badgeDot = style({
  animation: 'pulse 1.8s cubic-bezier(0.4, 0, 0.6, 1) infinite',
  background: colors.secondary,
  borderRadius: 'var(--jellypilot-radius-full)',
  height: 'var(--jellypilot-space-1_5)',
  width: 'var(--jellypilot-space-1_5)',
});

export const badgeText = style([
  atomic({
    color: 'var(--jellypilot-color-secondary)',
    fontSize: 'var(--jellypilot-font-size-10)',
    fontWeight: 'bold',
    gap: 2,
  }),
  {
    letterSpacing: '0.18em',
    textTransform: 'uppercase',
  },
]);

export const appTitle = style({
  color: colors.onSurface,
  fontFamily: 'var(--jellypilot-font-display)',
  fontSize: 'var(--jellypilot-font-size-45)',
  fontWeight: 'var(--jellypilot-font-weight-bold)',
  lineHeight: 'var(--jellypilot-line-height-52)',
});

export const appDescription = style([
  atomic({
    marginX: 'auto',
    marginTop: 2,
    color: 'var(--jellypilot-color-on-surface-variant)',
    fontSize: 'var(--jellypilot-font-size-16)',
    lineHeight: 24,
  }),
  {
    maxWidth: '28rem',
  },
]);

export const footer = atomic({
  marginTop: 8,
});
