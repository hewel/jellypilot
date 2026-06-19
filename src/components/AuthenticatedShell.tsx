import { Link, Outlet, useMatch } from '@tanstack/solid-router';
import { Activity, Library, MonitorPlay, Settings } from 'lucide-solid';
import { Show, createResource, createSignal, onCleanup, onMount } from 'solid-js';

import { commands, events } from '../bindings';
import type { ConnectionState, NowPlayingState } from '../bindings';
import { Button, StatusBadge } from './ui';

const navItems: {
  href: '/library' | '/now-playing' | '/settings' | '/diagnostics';
  label: string;
  Icon: typeof Library;
}[] = [
  { Icon: Library, href: '/library', label: 'Library' },
  {
    Icon: MonitorPlay,
    href: '/now-playing',
    label: 'Now Playing',
  },
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

function statusText(status?: NowPlayingState['status']) {
  switch (status) {
    case 'playing': {
      return 'Playing';
    }
    case 'paused': {
      return 'Paused';
    }
    case 'idle': {
      return 'MPV idle';
    }
    case 'offline': {
      return 'Player offline';
    }
    default: {
      return 'Playback unknown';
    }
  }
}

function ShellHeader(props: { connection: ConnectionState | undefined }) {
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
            <span class="brand-type text-title-large from-on-surface via-on-surface to-primary bg-gradient-to-r bg-clip-text text-transparent">
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

      {/* Server Status Panel */}
      <div class="border-outline-variant/20 flex items-center gap-3 border-t px-2 pt-2 lg:border-t-0 lg:pt-0">
        <Show
          when={props.connection}
          fallback={
            <div class="text-on-surface-variant/60 flex items-center gap-2.5">
              <span class="bg-outline-variant h-2 w-2 animate-pulse rounded-full" />
              <span class="text-body-small font-semibold">Connecting...</span>
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

function CompactNowPlayingSummary() {
  const [state, setState] = createSignal<NowPlayingState | null>(null);

  onMount(() => {
    void commands.nowPlayingGetState().then((result) => {
      if (result.status === 'ok') {
        setState(result.data);
      }
    });

    let disposed = false;
    let cleanup: (() => void) | undefined;
    events.nowPlayingChanged
      .listen((event) => setState(event.payload.state))
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          cleanup = unlisten;
        }
      });

    onCleanup(() => {
      disposed = true;
      cleanup?.();
    });
  });

  const title = () => state()?.media?.name ?? 'No active playback';
  const subtitle = () => {
    const media = state()?.media;
    if (!media) {
      return 'External MPV is ready for Jellyfin commands';
    }
    if (media.seriesName) {
      const episode =
        media.seasonNumber && media.episodeNumber
          ? `S${media.seasonNumber.toString().padStart(2, '0')}E${media.episodeNumber.toString().padStart(2, '0')}`
          : media.itemType;
      return `${media.seriesName} · ${episode}`;
    }
    return media.itemType;
  };

  return (
    <aside
      class="card-filled flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between"
      aria-label="Compact Now Playing"
    >
      <div class="flex min-w-0 items-center gap-3">
        <div class="border-secondary/30 bg-secondary-container/25 text-secondary flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border">
          <MonitorPlay class="h-5 w-5" />
        </div>
        <div class="min-w-0">
          <p class="text-label-small">Now Playing</p>
          <p class="text-title-medium truncate">{title()}</p>
          <p class="text-body-small truncate">{subtitle()}</p>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-3">
        <StatusBadge
          variant={
            state()?.status === 'playing' || state()?.status === 'paused' ? 'success' : 'neutral'
          }
        >
          {statusText(state()?.status)}
        </StatusBadge>
        <Button
          href="/now-playing"
          variant="secondary"
          class="rounded-full"
          leadingIcon={<MonitorPlay class="h-4 w-4" />}
        >
          Open Now Playing
        </Button>
      </div>
    </aside>
  );
}

export default function AuthenticatedShell() {
  const [connection] = createResource(() => commands.jellyfinGetState());
  const nowPlayingMatch = useMatch({
    from: '/_authenticated/now-playing',
    shouldThrow: false,
  });
  const showCompactNowPlaying = () => nowPlayingMatch() === undefined;

  return (
    <div class="console-shell">
      <div class="mx-auto flex w-full flex-col gap-5">
        <ShellHeader connection={connection()} />
        <div class="flex min-w-0 flex-col gap-6">
          <Show when={showCompactNowPlaying()}>
            <CompactNowPlayingSummary />
          </Show>
          <main class="animate-fade-in min-w-0">
            <Outlet />
          </main>
        </div>
      </div>
    </div>
  );
}
