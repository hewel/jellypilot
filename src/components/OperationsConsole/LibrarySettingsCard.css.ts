import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const toggle = style([
  sprinkles({
    display: 'flex',
    alignItems: 'flex-start',
    gap: '3',
    borderRadius: '2xl',
    p: '4',
    textAlign: 'left',
  }),
  {
    backdropFilter: 'blur(4px)',
    background: mix(vars.color.surfaceContainerHigh, 0.3),
    border: `1px solid ${mix(vars.color.outlineVariant, 0.6)}`,
    boxShadow: vars.shadow.inner,
    cursor: 'pointer',
    selectors: {
      '&:focus-visible': {
        outline: `2px solid ${vars.color.primary}`,
        outlineOffset: '2px',
      },
    },
  },
]);

export const checkbox = style([
  sprinkles({
    display: 'inlineFlex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: '0',
    color: 'onPrimary',
    fontSize: '11',
    lineHeight: 'none',
    borderRadius: 'lg',
  }),
  {
    background: vars.color.surfaceContainerHigh,
    border: `1px solid ${vars.color.outline}`,
    height: '1.375rem',
    marginTop: vars.space['0_5'],
    transitionDuration: vars.duration['200'],
    transitionProperty: 'background-color, border-color, box-shadow',
    width: '1.375rem',
    selectors: {
      '&:hover': {
        borderColor: mix(vars.color.primary, 0.6),
      },
    },
  },
]);

export const checkboxChecked = style({
  background: `linear-gradient(135deg, ${vars.color.primary}, ${vars.color.primaryGradientEnd})`,
  borderColor: vars.color.primary,
});

export const copy = sprinkles({
  minWidth: '0',
});

export const title = sprinkles({
  display: 'block',
  color: 'onSurface',
  fontSize: '14',
  lineHeight: '20',
  fontWeight: 'semibold',
});

export const description = style([
  sprinkles({
    mt: '1',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    color: mix(vars.color.onSurfaceVariant, 0.8),
  },
]);
