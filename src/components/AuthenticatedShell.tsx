import { createQuery } from '@tanstack/solid-query';
import { Outlet } from '@tanstack/solid-router';
import { Exit } from 'effect';

import { fetchConnectionState } from '../effects/connection';
import { queryKeys, runExit } from '../effects/query';
import * as styles from './AuthenticatedShell.styles';
import NowPlayingDrawer from './NowPlayingDrawer';
import SettingsModal from './SettingsModal';
import { ConsoleShell } from './ui';

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
      {/*
        Bottom padding reserves space so the fixed bottom-right floating cluster
        (Now Playing + Open Settings) never covers the last Library Browser items.
      */}
      <main class={styles.main}>
        <Outlet />
      </main>
      <div role="group" aria-label="Floating controls" class={styles.floatingControls}>
        <NowPlayingDrawer jellyfinConnected={jellyfinConnected()} />
        <SettingsModal />
      </div>
    </ConsoleShell>
  );
}
