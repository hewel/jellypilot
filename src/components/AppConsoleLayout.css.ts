import { atomic } from '@jellypilot/atomic-css';
import { projectBreakpoints, projectTheme } from '@jellypilot/ui/theme/project';
import { globalStyle, style } from '@vanilla-extract/css';

export const consoleShell = atomic({
  display: 'flex',
  minHeight: '100vh',
  flexDirection: 'column',
  justifyContent: 'space-between',
  color: 'var(--jellypilot-color-on-surface)',
  px: '0.625rem',
  py: '0.5rem',
});

const consoleContainerMotion = style({
  animation: `fadeIn ${projectTheme.duration['300']} ${projectTheme.easing.emphasized} forwards`,
});

globalStyle(`${consoleContainerMotion} > * + *`, {
  marginTop: projectTheme.space['6'],
});

export const consoleContainer = style([
  atomic({
    mx: 'auto',
    width: '100%',
  }),
  consoleContainerMotion,
]);

export const consoleGrid = style([
  atomic({
    display: 'grid',
    gap: '1.5rem',
  }),
  {
    gridTemplateColumns: '1fr',
    '@media': {
      [`screen and (min-width: ${projectBreakpoints.lg})`]: {
        gridTemplateColumns: 'minmax(0, 1.3fr) minmax(330px, 0.7fr)',
      },
    },
  },
]);
