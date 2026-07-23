import { createQuery } from '@tanstack/solid-query';
import { Outlet } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Show } from 'solid-js';
import { createSidebarPreferences } from '~utils/sidebarPreferences';
import { createSidebarWipe } from '~utils/sidebarWipe';

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
  const { collapsed } = createSidebarPreferences();
  const { wipe } = createSidebarWipe();

  return (
    <div class={styles.shell({ collapsed: collapsed() })}>
      <AppSidebar jellyfinConnected={jellyfinConnected()} />
      <Show when={wipe()} keyed>
        {(direction) => (
          <div
            aria-hidden="true"
            data-testid="sidebar-wipe"
            class={styles.sidebarWipe({ direction })}
          />
        )}
      </Show>
      <main class={styles.main({ glide: wipe() ?? undefined })}>
        <div class={styles.enter}>
          <Outlet />
        </div>
      </main>
    </div>
  );
}
