import { SegmentGroup } from '@ark-ui/solid/segment-group';
import type { VideoLibraryShortcut } from '@bindings';
import { useNavigate } from '@tanstack/solid-router';
import { House } from 'lucide-solid';
import { For, Show } from 'solid-js';

import { useLibraryNavbarControls } from './LibraryNavbarContext';

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
    <nav
      aria-label="Library navigation"
      class="border-outline-variant bg-surface-container-low/75 sticky top-2 z-30 rounded-2xl border shadow-xl backdrop-blur-md"
    >
      <div class="flex flex-row flex-wrap items-center justify-between gap-4">
        <SegmentGroup.Root
          value={props.activeValue}
          onValueChange={(details) => navigateToSegment(details.value)}
          class="relative flex min-w-0 flex-wrap gap-1 rounded-xl p-1"
        >
          <SegmentGroup.Indicator class="bg-secondary-container righ-(--right) absolute top-(--top) bottom-(--bottom) left-(--left) h-(--height) w-(--width) rounded-lg shadow-sm" />
          <For each={items()}>
            {(item) => (
              <SegmentGroup.Item
                value={item.value}
                class="text-on-surface-variant data-[state=checked]:text-on-secondary-container hover:text-on-surface relative z-10 inline-flex h-10 cursor-pointer items-center justify-center rounded-lg px-4 text-[14px] leading-5 font-bold transition-colors data-disabled:cursor-not-allowed data-disabled:opacity-50"
              >
                <SegmentGroup.ItemText>
                  <Show when={item.value === 'home'} fallback={item.label}>
                    <House class="h-4.5 w-4.5" />
                    <span class="sr-only">Home</span>
                  </Show>
                </SegmentGroup.ItemText>
                <SegmentGroup.ItemControl />
                <SegmentGroup.ItemHiddenInput />
              </SegmentGroup.Item>
            )}
          </For>
        </SegmentGroup.Root>

        <div ref={navbarControls.setPortalTarget} class="min-w-0 flex-1 xl:flex-none" />
      </div>
    </nav>
  );
}
