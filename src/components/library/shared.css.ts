import { style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { vars } from '../../styles/vars.css';

export const statusCard = style({
  display: 'grid',
  gap: vars.space['5'],
});

export const statusContent = sprinkles({
  display: 'flex',
  alignItems: 'flex-start',
  gap: '4',
});

export const statusIcon = style([
  sprinkles({
    display: 'flex',
    width: '12',
    height: '12',
    flexShrink: '0',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: '2xl',
    color: 'tertiary',
  }),
  {
    background: `color-mix(in srgb, ${vars.color.tertiaryContainer} 25%, transparent)`,
    border: `1px solid color-mix(in srgb, ${vars.color.tertiary} 30%, transparent)`,
  },
]);

export const iconMd = sprinkles({
  width: '6',
  height: '6',
});

export const statusCopy = style({
  display: 'grid',
  gap: vars.space['2'],
});

export const statusTitle = style([
  sprinkles({
    fontSize: '24',
    lineHeight: '32',
    fontWeight: 'bold',
  }),
  {
    fontFamily: vars.font.display,
  },
]);

export const statusDescription = sprinkles({
  color: 'onSurfaceVariant',
  fontSize: '14',
  lineHeight: '20',
});

export const row = style({
  display: 'grid',
  gap: vars.space['3'],
});

export const rowTitle = sprinkles({
  color: 'onSurface',
  fontSize: '22',
  lineHeight: '28',
  fontWeight: 'bold',
});

export const videoGrid = style({
  display: 'grid',
  gap: vars.space['3'],
  '@media': {
    'screen and (min-width: 640px)': {
      gridTemplateColumns: 'repeat(3, minmax(0, 1fr))',
    },
    'screen and (min-width: 1280px)': {
      gridTemplateColumns: 'repeat(4, minmax(0, 1fr))',
    },
    'screen and (min-width: 1536px)': {
      gridTemplateColumns: 'repeat(6, minmax(0, 1fr))',
    },
  },
});

export const subtitleLink = style({
  color: vars.color.secondary,
  textDecoration: 'none',
  textUnderlineOffset: vars.space['1'],
  selectors: {
    '&:hover': {
      textDecoration: 'underline',
    },
  },
});

export const userDataControls = style({
  display: 'grid',
  gap: vars.space['2'],
});

export const userDataActions = sprinkles({
  display: 'flex',
  flexWrap: 'wrap',
  gap: '3',
});

export const pillButton = style({
  borderRadius: vars.borderRadius.full,
});

export const favoriteSelected = style({
  borderColor: `color-mix(in srgb, ${vars.color.error} 30%, transparent)`,
});

export const playedSelected = style({
  borderColor: `color-mix(in srgb, ${vars.color.tertiary} 30%, transparent)`,
});

export const iconSm = sprinkles({
  width: '4',
  height: '4',
});

export const favoriteIcon = style({
  color: vars.color.onSurfaceVariant,
});

export const favoriteIconSelected = style({
  color: vars.color.error,
  fill: vars.color.error,
});

export const playedIcon = style({
  color: vars.color.onSurfaceVariant,
});

export const playedIconSelected = style({
  color: vars.color.tertiary,
  fontWeight: vars.fontWeight.bold,
});

export const spinIcon = style([
  sprinkles({
    width: '4',
    height: '4',
    color: 'secondary',
  }),
  {
    animation: 'spin 1s linear infinite',
  },
]);

export const errorText = sprinkles({
  color: 'error',
  fontSize: '12',
  lineHeight: '16',
});
