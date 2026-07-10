import type { VideoLibraryShortcut } from '@bindings';
import { SegmentedControl } from '@jellypilot/ui';
import { useNavigate } from '@tanstack/solid-router';
import { House } from 'lucide-solid';

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

  const selectedValue = () =>
    items().some((item) => item.value === props.activeValue) ? props.activeValue : 'home';

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
        <SegmentedControl
          aria-label="Library navigation"
          class={styles.segments}
          value={selectedValue()}
          items={items().map((item) =>
            item.value === 'home'
              ? {
                  value: item.value,
                  label: (
                    <>
                      <House class={styles.homeIcon} />
                      <span class={styles.srOnly}>Home</span>
                    </>
                  ),
                }
              : {
                  value: item.value,
                  label: item.label,
                },
          )}
          onValueChange={navigateToSegment}
        />

        <div ref={navbarControls.setPortalTarget} class={styles.portalTarget} />
      </div>
    </nav>
  );
}
