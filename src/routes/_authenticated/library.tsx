import { Outlet, createFileRoute } from '@tanstack/solid-router';

import * as styles from './library.styles';

export const Route = createFileRoute('/_authenticated/library')({
  component: () => (
    <div class={styles.stack}>
      <Outlet />
    </div>
  ),
});
