import type { VideoLibraryShortcut } from '@bindings';
import { createQuery } from '@tanstack/solid-query';
import { Link, useLocation } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Film, House, Tv } from 'lucide-solid';
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

import * as styles from './AppSidebar.styles';
import NowPlayingDrawer from './NowPlayingDrawer';
import SettingsModal from './SettingsModal';

export interface AppSidebarProps {
  jellyfinConnected: boolean;
}

interface SidebarItem {
  value: string;
  label: string;
  target: string;
  icon: JSX.Element;
  artworkImageId: string | null;
}

export default function AppSidebar(props: AppSidebarProps) {
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
  const browsePathMatch = createMemo(() =>
    /^\/library\/(movies|tvshows)\/([^/]+)$/.exec(normalizedPathname()),
  );
  const detailItemId = createMemo(
    () => /^\/library\/(?:items|shows)\/([^/]+)$/.exec(normalizedPathname())?.[1] ?? null,
  );
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
    const browse = browsePathMatch();
    if (browse) return `${browse[1]}:${browse[2]}`;
    const shortcut = detailShortcut();
    return shortcut ? `${shortcut.collectionType}:${shortcut.id}` : null;
  });

  const items = (): SidebarItem[] => [
    {
      value: 'home',
      label: 'Home',
      target: '/library',
      icon: <House class={styles.itemIcon} />,
      artworkImageId: null,
    },
    ...shortcuts().map((shortcut: VideoLibraryShortcut) => ({
      value: `${shortcut.collectionType}:${shortcut.id}`,
      label: shortcut.name,
      target: `/library/${shortcut.collectionType}/${shortcut.id}`,
      artworkImageId: shortcut.artworkImageId,
      icon:
        shortcut.collectionType === 'tvshows' ? (
          <Tv class={styles.itemIcon} />
        ) : (
          <Film class={styles.itemIcon} />
        ),
    })),
  ];

  return (
    <nav aria-label="Sidebar" class={styles.nav}>
      <p class={styles.sectionLabel}>Library</p>
      <ul class={styles.list}>
        <For each={items()}>
          {(item) => (
            <li>
              <Link
                to={item.target}
                activeOptions={{ exact: true, includeSearch: false, includeHash: false }}
                class={styles.item}
                data-active={activeValue() === item.value ? '' : undefined}
                aria-current={activeValue() === item.value ? 'page' : undefined}
              >
                <SidebarItemThumb artworkImageId={item.artworkImageId} fallbackIcon={item.icon} />
                <span class={styles.itemLabel}>{item.label}</span>
              </Link>
            </li>
          )}
        </For>
      </ul>
      <div class={styles.footer}>
        <NowPlayingDrawer jellyfinConnected={props.jellyfinConnected} />
        <SettingsModal />
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
