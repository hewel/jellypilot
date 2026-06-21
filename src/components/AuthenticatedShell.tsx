import { Outlet } from '@tanstack/solid-router';
import { Effect, Exit } from 'effect';
import { createResource } from 'solid-js';

import { fetchConnectionState } from '../effects/connection';
import NowPlayingDrawer from './NowPlayingDrawer';
import SettingsModal from './SettingsModal';
import { ConsoleShell } from './ui';

export default function AuthenticatedShell() {
  const [connection] = createResource(async () => {
    const exit = await Effect.runPromiseExit(fetchConnectionState());
    return Exit.isSuccess(exit) ? exit.value : undefined;
  });

  return (
    <ConsoleShell>
      {/*
        Bottom padding reserves space so the fixed bottom-right floating cluster
        (Now Playing + Open Settings) never covers the last Library Browser items.
      */}
      <main class="text-on-surface mx-auto flex w-full animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] flex-col pb-40">
        <Outlet />
      </main>
      <div class="fixed right-4 bottom-4 z-40 flex flex-col items-end gap-3">
        <NowPlayingDrawer jellyfinConnected={connection()?.connected ?? false} />
        <SettingsModal />
      </div>
    </ConsoleShell>
  );
}
