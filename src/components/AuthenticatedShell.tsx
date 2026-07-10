import { createQuery } from '@tanstack/solid-query';
import { Outlet } from '@tanstack/solid-router';
import { Exit } from 'effect';

import { fetchConnectionState } from '../effects/connection';
import { queryKeys, runExit } from '../effects/query';
import { ConsoleShell } from './AppConsoleLayout';
import NowPlayingDrawer from './NowPlayingDrawer';
import SettingsModal from './SettingsModal';
import ThemeCycleControl from './ThemeCycleControl';

import * as styles from './AuthenticatedShell.css';

export default function AuthenticatedShell() {
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState()),
  }));
  const jellyfinConnected = () =>
    connectionQuery.data && Exit.isSuccess(connectionQuery.data)
      ? connectionQuery.data.value.connected
      : false;

  return (
    <ConsoleShell>
      <main class={styles.main}>
        <Outlet />
      </main>
      <div role="group" aria-label="Floating controls" class={styles.floatingControls}>
        <NowPlayingDrawer jellyfinConnected={jellyfinConnected()} />
        <ThemeCycleControl />
        <SettingsModal />
      </div>
    </ConsoleShell>
  );
}
