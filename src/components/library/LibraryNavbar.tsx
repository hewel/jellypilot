import { SegmentGroup } from '@ark-ui/solid/segment-group';
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

  const navigateToSegment = (value: string | null) => {
    const target = items().find((item) => item.value === value)?.target;

    if (!target) {
      return;
    }

    void navigate({ to: target });
  };

  return (
    <nav aria-label="Library navigation" class={styles.root}>
      <div class={styles.inner}>
        <SegmentGroup.Root
          value={props.activeValue}
          onValueChange={(details) => navigateToSegment(details.value)}
          class={styles.segments}
        >
          <SegmentGroup.Indicator class={styles.indicator} />
          <For each={items()}>
            {(item) => (
              <SegmentGroup.Item value={item.value} class={styles.item}>
                <SegmentGroup.ItemText>
                  <Show when={item.value === 'home'} fallback={item.label}>
                    <House class={styles.homeIcon} />
                    <span class={styles.srOnly}>Home</span>
                  </Show>
                </SegmentGroup.ItemText>
                <SegmentGroup.ItemControl />
                <SegmentGroup.ItemHiddenInput />
              </SegmentGroup.Item>
            )}
          </For>
        </SegmentGroup.Root>

        <div ref={navbarControls.setPortalTarget} class={styles.portalTarget} />
      </div>
    </nav>
  );
}
