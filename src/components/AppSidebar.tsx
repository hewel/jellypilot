import type { VideoLibraryShortcut } from '@bindings';
import { createQuery } from '@tanstack/solid-query';
import { Link, useLocation, useParams } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Film, House, PanelLeftClose, PanelLeftOpen, Tv } from 'lucide-solid';
import { For, Show, createEffect, createMemo, createSignal, type JSX } from 'solid-js';
import { fetchConnectionState } from '~effects/connection';
import { fetchLibraryShortcuts, fetchVideoItemShortcut } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';
import { imageSource } from '~utils/imageSource';
import { createSidebarPreferences } from '~utils/sidebarPreferences';
import { createSidebarWipe, startSidebarWipe } from '~utils/sidebarWipe';

import * as styles from './AppSidebar.styles';
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
  artworkImageId: string | null;
}

export default function AppSidebar(props: AppSidebarProps) {
  const { collapsed, setCollapsed } = createSidebarPreferences();
  const { wipe } = createSidebarWipe();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
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
      artworkImageId: shortcut.artworkImageId,
      icon:
        shortcut.collectionType === 'tvshows' ? (
          <Tv class={styles.itemIcon} />
        ) : (
          <Film class={styles.itemIcon} />
        ),
    }));

  return (
    <nav
      aria-label="Sidebar"
      class={styles.nav({ collapsed: collapsed() })}
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
      <p class={styles.sectionLabel({ collapsed: collapsed() })}>Library</p>
      <ul class={styles.list}>
        <li>
          <Link
            to="/library"
            activeOptions={{ exact: true, includeSearch: false, includeHash: false }}
            class={styles.item({ collapsed: collapsed() })}
            data-active={activeValue() === 'home' ? '' : undefined}
            aria-current={activeValue() === 'home' ? 'page' : undefined}
          >
            <SidebarItemThumb
              artworkImageId={null}
              fallbackIcon={<House class={styles.itemIcon} />}
            />
            <span class={styles.itemLabel({ collapsed: collapsed() })}>Home</span>
          </Link>
        </li>
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
                <SidebarItemThumb artworkImageId={item.artworkImageId} fallbackIcon={item.icon} />
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

function SidebarItemThumb(props: { artworkImageId: string | null; fallbackIcon: JSX.Element }) {
  const [imageFailed, setImageFailed] = createSignal(false);
  createEffect(() => {
    props.artworkImageId;
    setImageFailed(false);
  });
  return (
    <Show when={!imageFailed() ? props.artworkImageId : null} fallback={props.fallbackIcon}>
      {(imageId) => (
        <img
          src={imageSource(imageId())}
          alt=""
          aria-hidden="true"
          class={styles.itemThumb}
          onError={() => setImageFailed(true)}
        />
      )}
    </Show>
  );
}
