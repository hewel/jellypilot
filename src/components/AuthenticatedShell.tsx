import { createQuery } from '@tanstack/solid-query';
import { Outlet } from '@tanstack/solid-router';
import { Exit } from 'effect';

import { fetchConnectionState } from '../effects/connection';
import { queryKeys, runExit } from '../effects/query';
import AppSidebar from './AppSidebar';
import * as styles from './AuthenticatedShell.styles';

export default function AuthenticatedShell() {
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
  }));
  const jellyfinConnected = () =>
    connectionQuery.data && Exit.isSuccess(connectionQuery.data)
      ? connectionQuery.data.value.connected
      : false;

  return (
    <div class={styles.shell}>
      <AppSidebar jellyfinConnected={jellyfinConnected()} />
      <main class={styles.main}>
        <Outlet />
      </main>
    </div>
  );
}
