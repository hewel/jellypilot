import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

const mix = (color: string, opacity: number) =>
  `color-mix(in srgb, ${color} ${Math.round(opacity * 100)}%, transparent)`;

export const stack = style({
  display: 'grid',
  gap: vars.space['3'],
});

export const profile = style([
  sprinkles({
    borderRadius: '2xl',
    p: '4',
  }),
  {
    background: mix(vars.color.surfaceContainerHigh, 0.3),
    border: `1px solid ${vars.color.outlineVariant}`,
  },
]);

export const activeProfile = style({
  borderColor: mix(vars.color.secondary, 0.7),
  boxShadow: '0 0 0 1px rgba(129, 140, 248, 0.25)',
});

export const warningProfile = style({
  borderColor: mix(vars.color.warning, 0.6),
});

export const profileInner = style({
  display: 'flex',
  flexDirection: 'column',
  gap: vars.space['3'],
  '@media': {
    'screen and (min-width: 640px)': {
      alignItems: 'flex-start',
      flexDirection: 'row',
      justifyContent: 'space-between',
    },
  },
});

export const copy = sprinkles({
  minWidth: '0',
});

export const titleRow = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: '2',
});

export const name = sprinkles({
  color: 'onSurface',
  fontSize: '15',
  lineHeight: '22',
  fontWeight: 'bold',
});

export const pill = style([
  sprinkles({
    borderRadius: 'full',
    px: '2',
    py: '0_5',
    fontSize: '10',
    lineHeight: '14',
    fontWeight: 'bold',
  }),
  {
    border: `1px solid ${vars.color.outlineVariant}`,
    color: vars.color.onSurfaceVariant,
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
  },
]);

export const activePill = style({
  background: mix(vars.color.secondary, 0.15),
  borderColor: 'transparent',
  color: vars.color.secondary,
});

export const url = style([
  sprinkles({
    mt: '1',
    color: 'secondary',
    fontSize: '12',
    lineHeight: '16',
  }),
  {
    fontFamily: vars.font.mono,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
]);

export const user = sprinkles({
  mt: '1',
  display: 'flex',
  alignItems: 'center',
  gap: '1_5',
  color: 'onSurfaceVariant',
  fontSize: '12',
  lineHeight: '16',
});

export const warning = sprinkles({
  mt: '2',
  display: 'flex',
  alignItems: 'flex-start',
  gap: '1_5',
  color: 'warning',
  fontSize: '12',
  lineHeight: '16',
  fontWeight: 'semibold',
});

export const warningIcon = style({
  flexShrink: 0,
  height: vars.space['3_5'],
  marginTop: vars.space['0_5'],
  width: vars.space['3_5'],
});

export const actions = sprinkles({
  display: 'flex',
  flexShrink: '0',
  flexWrap: 'wrap',
  gap: '2',
});

export const footer = sprinkles({
  mt: '5',
});
