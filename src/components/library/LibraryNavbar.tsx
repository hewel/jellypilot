import type { VideoLibraryShortcut } from '@bindings';
import { useNavigate } from '@tanstack/solid-router';
import { House } from 'lucide-solid';
import { For, Show } from 'solid-js';

import { useLibraryNavbarControls } from './LibraryNavbarContext';

import * as styles from './LibraryNavbar.css';

export interface LibraryNavbarProps {
  shortcuts: VideoLibraryShortcut[];
  activeValue: string;
}

interface LibraryNavbarItem {
  value: string;
  label: string;
  target: string;
}

export default function LibraryNavbar(props: LibraryNavbarProps) {
  const navigate = useNavigate();
  const navbarControls = useLibraryNavbarControls();
  const items = (): LibraryNavbarItem[] => [
    { value: 'home', label: 'Home', target: '/library' },
    ...props.shortcuts.map((shortcut) => ({
      value: `${shortcut.collectionType}:${shortcut.id}`,
      label: shortcut.name,
      target: `/library/${shortcut.collectionType}/${shortcut.id}`,
    })),
  ];

  const navigateToSegment = (value: string) => {
    const target = items().find((item) => item.value === value)?.target;

    if (!target) {
      return;
    }

    void navigate({ to: target });
  };

  return (
    <nav aria-label="Library navigation" class={styles.root}>
      <div class={styles.inner}>
        <div class={styles.segments} role="radiogroup" aria-label="Library sections">
          <For each={items()}>
            {(item) => (
              <button
                type="button"
                role="radio"
                class={styles.item}
                aria-label={item.label}
                aria-checked={item.value === props.activeValue}
                data-state={item.value === props.activeValue ? 'checked' : 'unchecked'}
                aria-current={item.value === props.activeValue ? 'page' : undefined}
                onClick={() => navigateToSegment(item.value)}
              >
                <span>
                  <Show when={item.value === 'home'} fallback={item.label}>
                    <House class={styles.homeIcon} />
                    <span class={styles.srOnly}>Home</span>
                  </Show>
                </span>
              </button>
            )}
          </For>
        </div>

        <div ref={navbarControls.setPortalTarget} class={styles.portalTarget} />
      </div>
    </nav>
  );
}
