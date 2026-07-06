import LibraryNavbar from '@components/library/LibraryNavbar';
import { LibraryNavbarControlsProvider } from '@components/library/LibraryNavbarContext';
import { createQuery } from '@tanstack/solid-query';
import { Outlet, createFileRoute, useLocation } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Show, createMemo } from 'solid-js';
import { fetchConnectionState } from '~effects/connection';
import { fetchLibraryShortcuts } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';

import * as styles from './library.css';

export const Route = createFileRoute('/_authenticated/library')({
  component: LibraryLayoutRoute,
});

function LibraryLayoutRoute() {
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState()),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const shortcutsQuery = createQuery(() => ({
    queryKey: queryKeys.libraryShortcuts(sessionKey()),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchLibraryShortcuts()),
  }));
  const shortcuts = () =>
    shortcutsQuery.data && Exit.isSuccess(shortcutsQuery.data) ? shortcutsQuery.data.value : [];
  const pathname = useLocation({ select: (location) => location.pathname });
  const normalizedPathname = createMemo(() => pathname().replace(/\/$/, '') || '/');
  const browsePathMatch = createMemo(() =>
    /^\/library\/(movies|tvshows)\/([^/]+)$/.exec(normalizedPathname()),
  );
  const showNavbar = createMemo(
    () => normalizedPathname() === '/library' || browsePathMatch() !== null,
  );
  const activeValue = createMemo(() => {
    const match = browsePathMatch();
    return match ? `${match[1]}:${match[2]}` : 'home';
  });

  return (
    <LibraryNavbarControlsProvider>
      <div class={styles.stack}>
        <Show when={showNavbar()}>
          <LibraryNavbar shortcuts={shortcuts()} activeValue={activeValue()} />
        </Show>
        <Outlet />
      </div>
    </LibraryNavbarControlsProvider>
  );
}
