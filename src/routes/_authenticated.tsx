import { Outlet, createFileRoute, useNavigate } from '@tanstack/solid-router';
import { Show } from 'solid-js';
import { createAmbientGlow } from '~utils/ambientGlow';
import { createSidebarPreferences } from '~utils/sidebarPreferences';
import { createSidebarWipe } from '~utils/sidebarWipe';

import AppSidebar from '../components/AppSidebar';
import {
  AuthenticatedBootstrapProvider,
  useAuthenticatedBootstrap,
} from '../components/AuthenticatedBootstrap';
import { NowPlayingProvider } from '../components/NowPlayingProvider';
import { AUTHENTICATED_HOME_ROUTE, requireAuthenticatedShell } from '../router-guards';
import * as styles from './_authenticated.styles';

export const Route = createFileRoute('/_authenticated')({
  beforeLoad: requireAuthenticatedShell,
  component: AuthenticatedShell,
});

function AuthenticatedShell() {
  const navigate = useNavigate();
  return (
    <AuthenticatedBootstrapProvider
      onSessionChange={() => void navigate({ to: AUTHENTICATED_HOME_ROUTE, replace: true })}
    >
      <NowPlayingProvider>
        <AuthenticatedShellContent />
      </NowPlayingProvider>
    </AuthenticatedBootstrapProvider>
  );
}

function AuthenticatedShellContent() {
  const bootstrap = useAuthenticatedBootstrap();
  const glow = createAmbientGlow();
  const { collapsed } = createSidebarPreferences();
  const { wipe } = createSidebarWipe();

  return (
    <div
      class={styles.shell({ collapsed: collapsed() })}
      data-shell=""
      data-glow={glow.active() ? '' : undefined}
      onPointerOver={glow.onPointerOver}
      onPointerOut={glow.onPointerOut}
    >
      <div aria-hidden="true" class={styles.ambient}>
        <div class={styles.ambientGlow}>
          <div class={styles.ambientCore} />
        </div>
      </div>
      <AppSidebar jellyfinConnected={bootstrap.connected()} />
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
