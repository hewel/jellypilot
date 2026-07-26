import { css } from '@styled-system/css';
import { material } from '~styles/recipes';

export type StatusBadgeVariant = 'success' | 'info' | 'warning' | 'error' | 'neutral';

/**
 * Status feedback maps to the semantic status families (foreground,
 * container, on-container, indicator). Text always accompanies the LED, so
 * color is never the only signal. LEDs are steady: badge states are not
 * transient activity, so no pulse animation.
 */
export const statusBadge = (props: { variant: StatusBadgeVariant }) =>
  css(
    {
      display: 'inline-flex',
      alignItems: 'center',
      flexShrink: '[0]',
      gap: '1_5',
      borderRadius: 'full',
      px: '3',
      py: '1',
      fontSize: '11',
      fontWeight: 'bold',
      lineHeight: '16',
      borderStyle: 'solid',
      borderWidth: '1px',
      letterSpacing: '8',
      textTransform: 'uppercase',
      userSelect: 'none',
    },
    variantStyles[props.variant],
  );

const variantStyles = {
  success: {
    bg: 'successContainer',
    borderColor: 'success',
    color: 'onSuccessContainer',
  },
  info: {
    bg: 'infoContainer',
    borderColor: 'info',
    color: 'onInfoContainer',
  },
  warning: {
    bg: 'warningContainer',
    borderColor: 'warning',
    color: 'onWarningContainer',
  },
  error: {
    bg: 'errorContainer',
    borderColor: 'error',
    color: 'onErrorContainer',
  },
  neutral: {
    bg: 'neutralContainer',
    borderColor: 'neutral',
    color: 'onNeutralContainer',
    fontWeight: 'semibold',
  },
} as const;

/** Steady LED dot; the indicator treatment owns the pill shape. */
export const statusDot = (props: { variant: StatusBadgeVariant }) =>
  css(
    material.raw({ treatment: 'indicator' }),
    {
      height: '1_5',
      width: '1_5',
    },
    dotStyles[props.variant],
  );

const dotStyles = {
  success: { bg: 'successIndicator' },
  info: { bg: 'infoIndicator' },
  warning: { bg: 'warningIndicator' },
  error: { bg: 'errorIndicator' },
  neutral: { bg: 'neutralIndicator' },
} as const;
