import { createQuery } from '@tanstack/solid-query';
import { Outlet, createFileRoute } from '@tanstack/solid-router';
import { Effect, Exit } from 'effect';
import { Show, onMount } from 'solid-js';
import { setImageProxyBase } from '~utils/imageSource';
import { createSidebarPreferences } from '~utils/sidebarPreferences';
import { createSidebarWipe } from '~utils/sidebarWipe';

import AppSidebar from '../components/AppSidebar';
import { fetchConnectionState } from '../effects/connection';
import { fetchAppLocalServices } from '../effects/localServices';
import { queryKeys, runExit } from '../effects/query';
import { requireAuthenticatedShell } from '../router-guards';
import * as styles from './_authenticated.styles';

export const Route = createFileRoute('/_authenticated')({
  beforeLoad: requireAuthenticatedShell,
  component: AuthenticatedShell,
});

function AuthenticatedShell() {
  onMount(() => {
    void Effect.runPromiseExit(fetchAppLocalServices).then((exit) =>
      Exit.match(exit, {
        onFailure: () => setImageProxyBase(null),
        onSuccess: (services) => setImageProxyBase(services?.imageProxyBase ?? null),
      }),
    );
  });

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
    <div class={styles.shell({ collapsed: collapsed() })} data-shell="">
      <div aria-hidden="true" class={styles.ambient}>
        <div class={styles.ambientGlow}>
          <div class={styles.ambientCore} />
        </div>
      </div>
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
