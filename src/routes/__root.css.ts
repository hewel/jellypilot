import { atomic } from '@jellypilot/atomic-css';
import { style } from '@vanilla-extract/css';

export const viewport = style([
  atomic({
    height: '100vh',
    overflow: 'auto',
    width: '100%',
  }),
  {
    overscrollBehavior: 'contain',
  },
]);

export const content = atomic({
  minWidth: '0',
});
