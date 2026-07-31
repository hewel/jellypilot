import type { VideoLibraryShortcut } from '@bindings';
import { createQuery } from '@tanstack/solid-query';
import { Link, useLocation, useParams } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Film, House, PanelLeftClose, PanelLeftOpen, Tv } from 'lucide-solid';
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import type { JSX } from 'solid-js';
import { fetchLibraryShortcuts, fetchVideoItemShortcut } from '~effects/library';
import { isLibrarySessionKeyConnected, queryKeys, runExit } from '~effects/query';
import { createSidebarPreferences } from '~utils/sidebarPreferences';
import { createSidebarWipe, startSidebarWipe } from '~utils/sidebarWipe';

import * as styles from './AppSidebar.styles';
import { useAuthenticatedBootstrap } from './AuthenticatedBootstrap';
import LibrarySearchBar from './LibrarySearchBar';
import NowPlayingDrawer from './NowPlayingDrawer';
import SettingsModal from './SettingsModal';
import { Button } from './ui';

export interface AppSidebarProps {
  jellyfinConnected: boolean;
}

interface SidebarShortcutItem {
  value: string;
  label: string;
  collectionType: string;
  libraryId: string;
  icon: JSX.Element;
}

const DESKTOP_MEDIA_QUERY = '(min-width: 64rem)';

export default function AppSidebar(props: AppSidebarProps) {
  const { collapsed, setCollapsed } = createSidebarPreferences();
  const { wipe } = createSidebarWipe();
  const bootstrap = useAuthenticatedBootstrap();
  const sessionKey = bootstrap.sessionKey;
  const shortcutsQuery = createQuery(() => ({
    queryKey: queryKeys.libraryShortcuts(sessionKey()),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchLibraryShortcuts),
  }));
  const shortcuts = () =>
    shortcutsQuery.data && Exit.isSuccess(shortcutsQuery.data) ? shortcutsQuery.data.value : [];

  const pathname = useLocation({ select: (location) => location.pathname });
  const normalizedPathname = createMemo(() => pathname().replace(/\/$/, '') || '/');
  const routeParams = useParams({ strict: false });
  const browseParams = createMemo(() => {
    const { collectionType, libraryId } = routeParams();
    return collectionType !== undefined && libraryId !== undefined
      ? { collectionType, libraryId }
      : null;
  });
  const detailItemId = createMemo(() => routeParams().itemId ?? routeParams().seriesId ?? null);
  const itemShortcutQuery = createQuery(() => ({
    queryKey: queryKeys.libraryItemShortcut(sessionKey(), detailItemId() ?? ''),
    enabled: isLibrarySessionKeyConnected(sessionKey()) && detailItemId() !== null,
    queryFn: () => runExit(fetchVideoItemShortcut(detailItemId() ?? '')),
  }));
  const detailShortcut = () =>
    itemShortcutQuery.data && Exit.isSuccess(itemShortcutQuery.data)
      ? itemShortcutQuery.data.value
      : null;

  const activeValue = createMemo((): string | null => {
    if (normalizedPathname() === '/library') return 'home';
    const browse = browseParams();
    if (browse) return `${browse.collectionType}:${browse.libraryId}`;
    const shortcut = detailShortcut();
    return shortcut ? `${shortcut.collectionType}:${shortcut.id}` : null;
  });

  const shortcutItems = (): SidebarShortcutItem[] =>
    shortcuts().map((shortcut: VideoLibraryShortcut) => ({
      value: `${shortcut.collectionType}:${shortcut.id}`,
      label: shortcut.name,
      collectionType: shortcut.collectionType,
      libraryId: shortcut.id,
      icon:
        shortcut.collectionType === 'tvshows' ? (
          <Tv class={styles.itemIcon} />
        ) : (
          <Film class={styles.itemIcon} />
        ),
    }));

  const [desktop, setDesktop] = createSignal(false);
  const [searchExpanded, setSearchExpanded] = createSignal(false);

  onMount(() => {
    if (typeof window.matchMedia !== 'function') {
      return;
    }
    const query = window.matchMedia(DESKTOP_MEDIA_QUERY);
    setDesktop(query.matches);
    const handleChange = (event: MediaQueryListEvent) => setDesktop(event.matches);
    query.addEventListener('change', handleChange);
    onCleanup(() => query.removeEventListener('change', handleChange));
  });

  // Navigation always closes the temporary narrow search expansion.
  createEffect(() => {
    pathname();
    setSearchExpanded(false);
  });

  const searchCollapsed = () => (collapsed() || !desktop()) && !searchExpanded();

  const handleSearchExpand = () => {
    if (desktop()) {
      // Durable preference change: the desktop sidebar stays expanded.
      if (collapsed()) {
        setCollapsed(false);
        startSidebarWipe(false);
      }
      return;
    }
    setSearchExpanded(true);
  };

  const handleSearchCollapse = () => {
    setSearchExpanded(false);
  };

  return (
    <nav
      aria-label="Sidebar"
      class={styles.nav({ collapsed: collapsed(), searchExpanded: searchExpanded() })}
      data-sidebar=""
      data-wiping={wipe() === null ? undefined : 'true'}
    >
      <div class={styles.header}>
        <Button
          type="button"
          variant="icon"
          size="row"
          aria-label={collapsed() ? 'Expand sidebar' : 'Collapse sidebar'}
          aria-expanded={!collapsed()}
          onClick={() => {
            const next = !collapsed();
            setCollapsed(next);
            startSidebarWipe(next);
          }}
          class={styles.collapseToggle({ collapsed: collapsed() })}
        >
          <Show when={collapsed()} fallback={<PanelLeftClose class={styles.collapseToggleIcon} />}>
            <PanelLeftOpen class={styles.collapseToggleIcon} />
          </Show>
          <span class={styles.collapseToggleLabel({ collapsed: collapsed() })}>
            <Show when={collapsed()} fallback="Collapse">
              Expand
            </Show>
          </span>
        </Button>
      </div>
      <div class={styles.searchSlot}>
        <LibrarySearchBar
          sessionKey={sessionKey()}
          collapsed={searchCollapsed()}
          onRequestExpand={handleSearchExpand}
          onRequestCollapse={handleSearchCollapse}
        />
      </div>
      <ul class={styles.list}>
        <li>
          <Link
            to="/library"
            activeOptions={{ exact: true, includeSearch: false, includeHash: false }}
            class={styles.item({ collapsed: collapsed() })}
            data-active={activeValue() === 'home' ? '' : undefined}
            aria-current={activeValue() === 'home' ? 'page' : undefined}
          >
            <span class={styles.itemIconSlot}>
              <House class={styles.itemIcon} />
            </span>
            <span class={styles.itemLabel({ collapsed: collapsed() })}>Home</span>
          </Link>
        </li>
      </ul>
      <p class={styles.sectionLabel({ collapsed: collapsed() })}>Library</p>
      <ul class={styles.list}>
        <For each={shortcutItems()}>
          {(item) => (
            <li>
              <Link
                to="/library/$collectionType/$libraryId"
                params={{ collectionType: item.collectionType, libraryId: item.libraryId }}
                activeOptions={{ exact: true, includeSearch: false, includeHash: false }}
                class={styles.item({ collapsed: collapsed() })}
                data-active={activeValue() === item.value ? '' : undefined}
                aria-current={activeValue() === item.value ? 'page' : undefined}
              >
                <span class={styles.itemIconSlot}>{item.icon}</span>
                <span class={styles.itemLabel({ collapsed: collapsed() })}>{item.label}</span>
              </Link>
            </li>
          )}
        </For>
      </ul>
      <div class={styles.footer}>
        <NowPlayingDrawer jellyfinConnected={props.jellyfinConnected} collapsed={collapsed()} />
        <SettingsModal collapsed={collapsed()} />
      </div>
    </nav>
  );
}
