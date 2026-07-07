import { style } from '@vanilla-extract/css';
import { recipe } from '@vanilla-extract/recipes';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const label = recipe({
  base: [
    sprinkles({
      display: 'block',
      color: 'onSurfaceVariant',
      fontSize: '12',
      fontWeight: 'bold',
      lineHeight: '16',
    }),
    style({
      letterSpacing: '0.05em',
      textTransform: 'uppercase',
    }),
  ],
  variants: {
    size: {
      compact: sprinkles({ mb: '2' }),
      standard: sprinkles({ mb: '1_5' }),
    },
  },
});

export const control = sprinkles({
  display: 'flex',
  width: 'full',
  alignItems: 'center',
});

export const trigger = recipe({
  base: [
    sprinkles({
      display: 'flex',
      width: 'full',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '2',
      color: 'onSurface',
    }),
    style({
      borderStyle: 'solid',
      borderWidth: '1px',
      outline: 'none',
      textAlign: 'left',
      transitionDuration: vars.duration['200'],
      transitionProperty: 'background-color, border-color, box-shadow',
      selectors: {
        '&:hover': {
          borderColor: mix(vars.color.secondary, 0.5),
        },
        '&:focus': {
          borderColor: vars.color.secondary,
        },
        '&:disabled': {
          cursor: 'not-allowed',
          opacity: 0.5,
        },
      },
    }),
  ],
  variants: {
    size: {
      compact: {
        background: vars.color.surfaceContainerLow,
        borderColor: vars.color.outlineVariant,
        borderRadius: vars.borderRadius.lg,
        height: vars.space['12'],
        paddingInline: vars.space['3'],
        selectors: {
          '&:focus': {
            boxShadow: `0 0 0 2px ${mix(vars.color.secondary, 0.25)}`,
          },
        },
      },
      standard: {
        background: mix(vars.color.surfaceContainerHighest, 0.3),
        borderColor: mix(vars.color.outlineVariant, 0.8),
        borderRadius: vars.borderRadius['2xl'],
        height: vars.space['14'],
        paddingInline: vars.space['4'],
        selectors: {
          '&:focus': {
            boxShadow: `0 0 0 4px ${mix(vars.color.secondary, 0.15)}`,
          },
        },
      },
    },
  },
});

export const content = style([
  sprinkles({
    overflowY: 'auto',
    borderRadius: 'lg',
    p: '2',
    boxShadow: '2xl',
    zIndex: '50',
  }),
  {
    backdropFilter: 'blur(12px)',
    background: vars.color.surfaceContainerLowest,
    border: `1px solid ${vars.color.outlineVariant}`,
    maxHeight: '15rem',
  },
]);

export const positioner = style({
  position: 'fixed',
  zIndex: vars.zIndex['50'],
});

export const item = style([
  sprinkles({
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    borderRadius: 'xl',
    color: 'onSurfaceVariant',
    fontSize: '14',
    lineHeight: '20',
  }),
  {
    cursor: 'pointer',
    padding: `${vars.space['2_5']} ${vars.space['3_5']}`,
    transitionDuration: vars.duration['200'],
    transitionProperty: 'background-color, color',
    selectors: {
      '&:hover': {
        background: vars.color.surfaceContainerHigh,
        color: vars.color.onSurface,
      },
      '&[data-disabled]': {
        cursor: 'not-allowed',
        opacity: 0.5,
      },
    },
  },
]);

export const itemText = sprinkles({ fontWeight: 'medium' });

export const valueText = sprinkles({
  minWidth: '0',
  color: 'onSurface',
  fontSize: '14',
  fontWeight: 'medium',
  lineHeight: '20',
});

export const indicator = style({});

export const indicatorIcon = style([
  sprinkles({
    color: 'onSurfaceVariant',
    height: '4',
    width: '4',
  }),
  {
    opacity: 0.7,
    selectors: {
      [`${indicator}[data-state="open"] &`]: {
        transform: 'rotate(180deg)',
      },
    },
    transitionDuration: vars.duration['200'],
    transitionProperty: 'transform',
  },
]);

export const truncate = style({
  minWidth: 0,
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
});

export const hiddenSelect = style({
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
