import { ScrollArea } from '@ark-ui/solid';
import { Outlet, createRootRoute } from '@tanstack/solid-router';
import type { Component } from 'solid-js';

const ScrollerWarpper: Component = () => (
  <ScrollArea.Root class="relative flex h-screen w-screen overflow-hidden">
    <ScrollArea.Viewport class="h-full w-full scrollbar-none overscroll-contain">
      <ScrollArea.Content>
        <Outlet />
      </ScrollArea.Content>
    </ScrollArea.Viewport>
    <ScrollArea.Scrollbar class="flex touch-none p-0.5 opacity-0 transition-colors duration-200 hover:opacity-100 data-scrolling:opacity-100 data-[orientation=horizontal]:h-3 data-[orientation=horizontal]:w-full data-[orientation=horizontal]:flex-col data-[orientation=vertical]:h-full data-[orientation=vertical]:w-3">
      <ScrollArea.Thumb class="bg-outline/55 hover:bg-outline relative flex-1 rounded-full transition-colors before:absolute before:-inset-1" />
    </ScrollArea.Scrollbar>
    <ScrollArea.Corner class="bg-surface-container-highest/70" />
  </ScrollArea.Root>
);

export const Route = createRootRoute({
  component: ScrollerWarpper,
});
