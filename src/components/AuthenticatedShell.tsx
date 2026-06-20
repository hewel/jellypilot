import { Link, Outlet } from '@tanstack/solid-router';
import { Activity, Library, Settings } from 'lucide-solid';
import { Show, createResource } from 'solid-js';

import { commands } from '../bindings';
import type { ConnectionState } from '../bindings';
import NowPlayingDrawer from './NowPlayingDrawer';
import { ConsoleShell } from './ui';

const navItems: {
  href: '/library' | '/settings' | '/diagnostics';
  label: string;
  Icon: typeof Library;
}[] = [
  { Icon: Library, href: '/library', label: 'Library' },
  { Icon: Settings, href: '/settings', label: 'Settings' },
  {
    Icon: Activity,
    href: '/diagnostics',
    label: 'Diagnostics',
  },
];

const navItemClass =
  'inline-flex min-h-11 shrink-0 items-center gap-2.5 rounded-lg lg:rounded-xl px-3.5 text-[14px] font-bold transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70';

const activeNavItemClass =
  'border border-primary/30 bg-primary-container/45 text-on-primary-container shadow-[inset_0_1px_1px_rgba(255,255,255,0.1),0_0_12px_rgba(79,70,229,0.15)]';

const inactiveNavItemClass =
  'border border-transparent text-on-surface-variant hover:border-outline-variant/50 hover:bg-surface-container-high/40 hover:text-on-surface';

function ShellHeader(props: {
  connection: ConnectionState | undefined;
  jellyfinConnected: boolean;
}) {
  return (
    <header class="border-outline-variant bg-surface-container-low/60 flex flex-col gap-3 rounded-2xl border p-3 shadow-xl backdrop-blur-md lg:flex-row lg:items-center lg:justify-between lg:rounded-[1.75rem] lg:p-4">
      {/* Brand Header */}
      <div class="flex items-center gap-2 px-2 py-1">
        <span class="relative flex h-3.5 w-3.5 items-center justify-center">
          <span class="bg-primary/40 absolute inline-flex h-full w-full animate-ping rounded-full opacity-75" />
          <span class="bg-primary relative inline-flex h-2.5 w-2.5 rounded-full shadow-[0_0_8px_var(--color-primary)]" />
        </span>
        <div class="flex flex-col">
          <div class="flex items-center gap-1.5">
            <span class="font-display from-on-surface via-on-surface to-primary bg-gradient-to-r bg-clip-text text-[22px] leading-[28px] font-bold tracking-tight text-transparent">
              JMSR
            </span>
            <span class="border-primary/20 bg-primary/5 text-primary rounded border px-1.5 py-0.5 text-[9px] font-black tracking-[0.2em] uppercase">
              v2
            </span>
          </div>
          <p class="text-on-surface-variant/70 -mt-0.5 text-[11px] font-bold tracking-[0.15em] uppercase">
            Control Room
          </p>
        </div>
      </div>

      {/* Navigation List */}
      <nav aria-label="JMSR areas" class="flex gap-2 overflow-x-auto lg:overflow-visible">
        {navItems.map(({ href, label, Icon }) => (
          <Link
            activeOptions={{ exact: false }}
            activeProps={{ class: activeNavItemClass }}
            inactiveProps={{ class: inactiveNavItemClass }}
            to={href}
            class={navItemClass}
          >
            <Icon class="h-4.5 w-4.5" />
            <span>{label}</span>
          </Link>
        ))}
      </nav>

      {/* Status Panel */}
      <div class="border-outline-variant/20 flex items-center gap-3 border-t px-2 pt-2 lg:border-t-0 lg:pt-0">
        <NowPlayingDrawer jellyfinConnected={props.jellyfinConnected} />
        <Show
          when={props.connection}
          fallback={
            <div class="text-on-surface-variant/60 flex items-center gap-2.5">
              <span class="bg-outline-variant h-2 w-2 animate-pulse rounded-full" />
              <span class="text-on-surface-variant/80 text-[12px] leading-[16px] font-semibold">
                Connecting...
              </span>
            </div>
          }
        >
          {(conn) => (
            <div class="flex items-center gap-3">
              <div class="flex flex-col text-left lg:text-right">
                <p class="text-on-surface truncate text-[12px] leading-tight font-bold">
                  {conn().userName || 'Guest User'}
                </p>
                <p class="text-on-surface-variant/80 mt-0.5 truncate text-[10px] leading-none font-semibold">
                  {conn().serverName || 'Jellyfin Server'}
                </p>
              </div>
              <div class="border-primary/20 bg-primary-container/30 text-primary font-display flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border text-xs font-black">
                {conn().userName?.charAt(0).toUpperCase() || 'J'}
              </div>
              <span
                class={`h-2 w-2 shrink-0 rounded-full ${
                  conn().connected
                    ? 'bg-tertiary animate-pulse shadow-[0_0_8px_var(--color-tertiary)]'
                    : 'bg-error shadow-[0_0_8px_var(--color-error)]'
                }`}
              />
            </div>
          )}
        </Show>
      </div>
    </header>
  );
}

export default function AuthenticatedShell() {
  const [connection] = createResource(() => commands.jellyfinGetState());

  return (
    <ConsoleShell>
      <div class="mx-auto flex w-full flex-col gap-5">
        <ShellHeader
          connection={connection()}
          jellyfinConnected={connection()?.connected ?? false}
        />
        <main class="min-w-0 animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards]">
          <Outlet />
        </main>
      </div>
    </ConsoleShell>
  );
}
