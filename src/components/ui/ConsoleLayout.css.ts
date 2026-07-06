import { globalStyle, style } from '@vanilla-extract/css';

import { sprinkles } from '../../styles/sprinkles.css';
import { breakpoints, vars } from '../../styles/vars.css';

export const consoleShell = sprinkles({
  display: 'flex',
  minHeight: 'dynamicScreenHeight',
  flexDirection: 'column',
  justifyContent: 'space-between',
  color: 'onSurface',
  px: '2_5',
  py: '2',
});

const consoleContainerMotion = style({
  animation: `fadeIn ${vars.duration['300']} ${vars.easing.emphasized} forwards`,
});

globalStyle(`${consoleContainerMotion} > * + *`, {
  marginTop: vars.space['6'],
});

export const consoleContainer = style([
  sprinkles({
    mx: 'auto',
    width: 'full',
  }),
  consoleContainerMotion,
]);

export const consoleGrid = style([
  sprinkles({
    display: 'grid',
    gap: '6',
  }),
  {
    gridTemplateColumns: '1fr',
    '@media': {
      [`screen and (min-width: ${breakpoints.lg})`]: {
        gridTemplateColumns: 'minmax(0, 1.3fr) minmax(330px, 0.7fr)',
      },
    },
  },
]);
