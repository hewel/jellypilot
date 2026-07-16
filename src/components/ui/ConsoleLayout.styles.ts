import { css } from '@styled-system/css';

export const consoleShell = css({
  display: 'flex',
  minHeight: '[100dvh]',
  flexDirection: 'column',
  justifyContent: 'space-between',
  color: 'onSurface',
  px: '2_5',
  py: '2',
});

export const consoleContainer = css({
  mx: 'auto',
  width: 'full',
  animation: '[fadeIn 300ms {easings.emphasized} forwards]',
  '& > * + *': {
    marginTop: '6',
  },
});

export const consoleGrid = css({
  display: 'grid',
  gap: '6',
  gridTemplateColumns: '[1fr]',
  lg: {
    gridTemplateColumns: '[minmax(0, 1.3fr) minmax(330px, 0.7fr)]',
  },
});
