import { atomic } from '@jellypilot/atomic-css';

export const card = atomic({ width: 'full' });

export const header = atomic({
  display: 'flex',
  items: 'center',
  justify: 'between',
  mb: 6,
});

export const title = atomic({
  display: 'flex',
  items: 'center',
  gap: 3,
});
