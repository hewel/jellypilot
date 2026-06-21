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
      <div
        role="group"
        aria-label="Floating controls"
        class="border-outline-variant/40 bg-surface-container-low/80 fixed right-4 bottom-4 z-100 flex flex-col items-center gap-2 rounded-3xl border p-1 shadow-2xl backdrop-blur-xl"
      >
        <NowPlayingDrawer jellyfinConnected={connection()?.connected ?? false} />
        <SettingsModal />
      </div>
    </ConsoleShell>
  );
}
