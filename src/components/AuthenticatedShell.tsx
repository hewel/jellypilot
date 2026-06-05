import { createListCollection } from '@ark-ui/solid/collection';
import { Select } from '@ark-ui/solid/select';
import { Link, Outlet, useMatch } from '@tanstack/solid-router';
import { Effect, Exit } from 'effect';
import {
  Activity,
  ChevronDown,
  Clapperboard,
  Film,
  Library,
  MonitorPlay,
  RefreshCw,
  Settings,
  Tv,
} from 'lucide-solid';
import {
  createResource,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
} from 'solid-js';
import {
  type ConnectionState,
  commands,
  events,
  type NowPlayingState,
  type VideoHome,
  type VideoHomeItem,
  type VideoItemDetail,
  type VideoLibraryItem,
  type VideoLibraryKind,
  type VideoLibraryPage,
  type VideoLibraryPlayedFilter,
  type VideoLibraryPlayMode,
  type VideoLibraryPlayRequest,
  type VideoLibraryShortcut,
  type VideoLibrarySort,
  type VideoSearchPage,
  type VideoSeason,
  type VideoSeasonEpisodes,
  type VideoSeasonEpisodesRequest,
  type VideoShowDetail,
  type VideoUserDataAction,
  type VideoUserDataUpdateRequest,
} from '../bindings';
import {
  commandFailureMessage,
  runTauriCommand,
  runTauriCommandRaw,
} from '../effects/commands';
import DiagnosticsPanel from './DiagnosticsPanel';
import { StatusBadge } from './ui';

type LibraryHomeState =
  | { kind: 'ready'; home: VideoHome; connection: ConnectionState }
  | { kind: 'empty'; connection: ConnectionState }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

type LibraryBrowseState =
  | { kind: 'ready'; page: VideoLibraryPage; items: VideoLibraryItem[] }
  | { kind: 'empty'; page: VideoLibraryPage }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

type LibrarySearchState =
  | { kind: 'ready'; page: VideoSearchPage; items: VideoLibraryItem[] }
  | { kind: 'empty'; page: VideoSearchPage }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

type LibraryDetailState =
  | { kind: 'ready'; detail: VideoItemDetail }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

type LibraryShowState =
  | { kind: 'ready'; detail: VideoShowDetail }
  | { kind: 'disconnected'; state: ConnectionState }
  | { kind: 'error'; message: string };

type SeasonEpisodesState =
  | { kind: 'ready'; page: VideoSeasonEpisodes }
  | { kind: 'empty'; page: VideoSeasonEpisodes }
  | { kind: 'error'; message: string };

const LIBRARY_BROWSE_PAGE_SIZE = 24;
const LIBRARY_SEARCH_PAGE_SIZE = 24;

const navItems: Array<{
  href: '/library' | '/now-playing' | '/settings' | '/diagnostics';
  label: string;
  Icon: typeof Library;
}> = [
  { href: '/library', label: 'Library', Icon: Library },
  {
    href: '/now-playing',
    label: 'Now Playing',
    Icon: MonitorPlay,
  },
  { href: '/settings', label: 'Settings', Icon: Settings },
  {
    href: '/diagnostics',
    label: 'Diagnostics',
    Icon: Activity,
  },
];

const navItemClass =
  'inline-flex min-h-11 shrink-0 items-center gap-2.5 rounded-lg lg:rounded-xl px-3.5 text-[14px] font-bold transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70';

const activeNavItemClass =
  'border border-primary/30 bg-primary-container/45 text-on-primary-container shadow-[inset_0_1px_1px_rgba(255,255,255,0.1),0_0_12px_rgba(79,70,229,0.15)]';

const inactiveNavItemClass =
  'border border-transparent text-on-surface-variant hover:border-outline-variant/50 hover:bg-surface-container-high/40 hover:text-on-surface';

function videoHomeIsEmpty(home: VideoHome) {
  return (
    home.continueWatching.length === 0 &&
    home.nextUp.length === 0 &&
    home.latestMovies.length === 0 &&
    home.latestEpisodes.length === 0 &&
    home.libraryShortcuts.length === 0
  );
}

async function fetchLibraryHome(): Promise<LibraryHomeState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const home = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryVideoHome()),
  );

  if (!Exit.isSuccess(home)) {
    return {
      kind: 'error',
      message: commandFailureMessage(home.cause, 'Could not load Video Home'),
    };
  }

  return videoHomeIsEmpty(home.value)
    ? { kind: 'empty', connection: connection.value }
    : { kind: 'ready', home: home.value, connection: connection.value };
}

async function fetchVideoLibraryPage(
  collectionType: VideoLibraryKind,
  libraryId: string,
  startIndex: number,
  sort: VideoLibrarySort,
  playedFilter: VideoLibraryPlayedFilter,
  favoritesOnly: boolean,
): Promise<LibraryBrowseState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const page = await Effect.runPromiseExit(
    runTauriCommand(() =>
      commands.libraryBrowseVideo({
        collectionType,
        libraryId,
        startIndex,
        limit: LIBRARY_BROWSE_PAGE_SIZE,
        sort,
        playedFilter,
        favoritesOnly,
      }),
    ),
  );

  if (!Exit.isSuccess(page)) {
    return {
      kind: 'error',
      message: commandFailureMessage(page.cause, 'Could not load Library page'),
    };
  }

  return page.value.items.length === 0
    ? { kind: 'empty', page: page.value }
    : { kind: 'ready', page: page.value, items: page.value.items };
}

async function fetchVideoSearchPage(
  query: string,
  startIndex: number,
): Promise<LibrarySearchState> {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) {
    return { kind: 'error', message: 'Search text is required' };
  }

  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const page = await Effect.runPromiseExit(
    runTauriCommand(() =>
      commands.librarySearchVideo({
        query: trimmedQuery,
        startIndex,
        limit: LIBRARY_SEARCH_PAGE_SIZE,
      }),
    ),
  );

  if (!Exit.isSuccess(page)) {
    return {
      kind: 'error',
      message: commandFailureMessage(page.cause, 'Could not search Library'),
    };
  }

  return page.value.items.length === 0
    ? { kind: 'empty', page: page.value }
    : { kind: 'ready', page: page.value, items: page.value.items };
}

async function fetchVideoItemDetail(
  itemId: string,
): Promise<LibraryDetailState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const detail = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryItemDetail(itemId)),
  );

  if (!Exit.isSuccess(detail)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        detail.cause,
        'Could not load item detail',
      ),
    };
  }

  return { kind: 'ready', detail: detail.value };
}

async function fetchVideoShowDetail(
  seriesId: string,
): Promise<LibraryShowState> {
  const connection = await Effect.runPromiseExit(
    runTauriCommandRaw(() => commands.jellyfinGetState()),
  );

  if (!Exit.isSuccess(connection)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        connection.cause,
        'Could not load Library state',
      ),
    };
  }

  if (!connection.value.connected) {
    return { kind: 'disconnected', state: connection.value };
  }

  const detail = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryShowDetail(seriesId)),
  );

  if (!Exit.isSuccess(detail)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        detail.cause,
        'Could not load show detail',
      ),
    };
  }

  return { kind: 'ready', detail: detail.value };
}

async function fetchSeasonEpisodes(
  request: VideoSeasonEpisodesRequest,
): Promise<SeasonEpisodesState> {
  const page = await Effect.runPromiseExit(
    runTauriCommand(() => commands.librarySeasonEpisodes(request)),
  );

  if (!Exit.isSuccess(page)) {
    return {
      kind: 'error',
      message: commandFailureMessage(
        page.cause,
        'Could not load season episodes',
      ),
    };
  }

  return page.value.episodes.length === 0
    ? { kind: 'empty', page: page.value }
    : { kind: 'ready', page: page.value };
}

async function startLibraryPlayback(
  request: VideoLibraryPlayRequest,
): Promise<string | null> {
  const result = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryPlay(request)),
  );

  if (!Exit.isSuccess(result)) {
    return commandFailureMessage(result.cause, 'Could not start playback');
  }

  return null;
}

async function updateLibraryUserData(
  request: VideoUserDataUpdateRequest,
): Promise<string | null> {
  const result = await Effect.runPromiseExit(
    runTauriCommand(() => commands.libraryUpdateUserData(request)),
  );

  if (!Exit.isSuccess(result)) {
    return commandFailureMessage(result.cause, 'Could not update user data');
  }

  return null;
}

function statusText(status?: NowPlayingState['status']) {
  switch (status) {
    case 'playing':
      return 'Playing';
    case 'paused':
      return 'Paused';
    case 'idle':
      return 'MPV idle';
    case 'offline':
      return 'Player offline';
    default:
      return 'Playback unknown';
  }
}

function ShellHeader(props: { connection: ConnectionState | undefined }) {
  return (
    <header class="flex flex-col gap-3 rounded-2xl lg:rounded-[1.75rem] border border-outline-variant bg-surface-container-low/60 p-3 shadow-xl backdrop-blur-md lg:flex-row lg:items-center lg:justify-between lg:p-4">
      {/* Brand Header */}
      <div class="flex items-center gap-2 px-2 py-1">
        <span class="relative flex h-3.5 w-3.5 items-center justify-center">
          <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary/40 opacity-75" />
          <span class="relative inline-flex h-2.5 w-2.5 rounded-full bg-primary shadow-[0_0_8px_var(--color-primary)]" />
        </span>
        <div class="flex flex-col">
          <div class="flex items-center gap-1.5">
            <span class="brand-type text-title-large bg-gradient-to-r from-on-surface via-on-surface to-primary bg-clip-text text-transparent">
              JMSR
            </span>
            <span class="text-[9px] font-black uppercase tracking-[0.2em] px-1.5 py-0.5 rounded border border-primary/20 bg-primary/5 text-primary">
              v2
            </span>
          </div>
          <p class="text-[11px] font-bold uppercase tracking-[0.15em] text-on-surface-variant/70 -mt-0.5">
            Control Room
          </p>
        </div>
      </div>

      {/* Navigation List */}
      <nav
        aria-label="JMSR areas"
        class="flex gap-2 overflow-x-auto lg:overflow-visible"
      >
        {navItems.map(({ href, label, Icon }) => {
          return (
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
          );
        })}
      </nav>

      {/* Server Status Panel */}
      <div class="flex items-center gap-3 px-2 border-t border-outline-variant/20 pt-2 lg:border-t-0 lg:pt-0">
        <Show
          when={props.connection}
          fallback={
            <div class="flex items-center gap-2.5 text-on-surface-variant/60">
              <span class="w-2 h-2 rounded-full bg-outline-variant animate-pulse" />
              <span class="text-body-small font-semibold">Connecting...</span>
            </div>
          }
        >
          {(conn) => (
            <div class="flex items-center gap-3">
              <div class="flex flex-col text-left lg:text-right">
                <p class="truncate text-[12px] font-bold text-on-surface leading-tight">
                  {conn().userName || 'Guest User'}
                </p>
                <p class="truncate text-[10px] font-semibold text-on-surface-variant/80 leading-none mt-0.5">
                  {conn().serverName || 'Jellyfin Server'}
                </p>
              </div>
              <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-primary/20 bg-primary-container/30 text-primary font-display font-black text-xs">
                {conn().userName?.charAt(0).toUpperCase() || 'J'}
              </div>
              <span
                class={`w-2 h-2 shrink-0 rounded-full ${
                  conn().connected
                    ? 'bg-tertiary shadow-[0_0_8px_var(--color-tertiary)] animate-pulse'
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
      if (result.status === 'ok') setState(result.data);
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
    if (!media) return 'External MPV is ready for Jellyfin commands';
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
        <div class="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-secondary/30 bg-secondary-container/25 text-secondary">
          <MonitorPlay class="h-5 w-5" />
        </div>
        <div class="min-w-0">
          <p class="text-label-small">Now Playing</p>
          <p class="truncate text-title-medium">{title()}</p>
          <p class="truncate text-body-small">{subtitle()}</p>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-3">
        <StatusBadge
          variant={
            state()?.status === 'playing' || state()?.status === 'paused'
              ? 'success'
              : 'neutral'
          }
        >
          {statusText(state()?.status)}
        </StatusBadge>
        <a href="/now-playing" class="btn-secondary rounded-full">
          <MonitorPlay class="h-4 w-4" />
          <span>Open Now Playing</span>
        </a>
      </div>
    </aside>
  );
}

export function LibraryLanding() {
  const [home, { refetch }] = createResource(fetchLibraryHome);
  const loadedHome = () => {
    const current = home();
    return current?.kind === 'ready' ? current.home : null;
  };
  const statusTitle = () => {
    const current = home();
    if (current?.kind === 'empty') return 'Video Home has no video rows yet';
    if (current?.kind === 'error') return 'Could not load Library state';
    return 'Library requires a live Jellyfin connection';
  };
  const statusDescription = () => {
    const current = home();
    if (current?.kind === 'empty') {
      return 'Jellyfin returned no Continue Watching, Next Up, latest video rows, or video library shortcuts for this user.';
    }
    if (current?.kind === 'error') return current.message;
    return 'Reconnect Jellyfin to browse video libraries. Saved Sessions remain available, but Library data is not cached offline.';
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p class="text-label-small text-secondary">Library Browser</p>
          <h1 class="text-headline-large">Library</h1>
          <p class="mt-2 max-w-2xl text-body-large">
            Video Home will use live Jellyfin data for Continue Watching, Next
            Up, latest video rows, and library shortcuts.
          </p>
        </div>
        <button
          type="button"
          class="btn-outlined rounded-full"
          onClick={() => void refetch()}
          disabled={home.loading}
        >
          <RefreshCw class="h-4 w-4" />
          <span>Retry Library</span>
        </button>
      </div>

      <LibrarySearchPanel />

      <Show
        when={!home.loading}
        fallback={<LibraryStatusPanel title="Loading Video Home" />}
      >
        <Show
          when={home()?.kind === 'ready'}
          fallback={
            <LibraryStatusPanel
              title={statusTitle()}
              description={statusDescription()}
            />
          }
        >
          <div class="space-y-6">
            <VideoHomeRow
              id="continue-watching"
              title="Continue Watching"
              items={loadedHome()?.continueWatching ?? []}
            />
            <VideoHomeRow
              id="next-up"
              title="Next Up"
              items={loadedHome()?.nextUp ?? []}
            />
            <VideoHomeRow
              id="latest-movies"
              title="Latest Movies"
              items={loadedHome()?.latestMovies ?? []}
            />
            <VideoHomeRow
              id="latest-episodes"
              title="Latest Episodes"
              items={loadedHome()?.latestEpisodes ?? []}
            />
            <LibraryShortcutRow
              shortcuts={loadedHome()?.libraryShortcuts ?? []}
            />
          </div>
        </Show>
      </Show>
    </div>
  );
}

function LibrarySearchPanel() {
  const [query, setQuery] = createSignal('');
  const [submittedQuery, setSubmittedQuery] = createSignal('');
  const [state, setState] = createSignal<LibrarySearchState | null>(null);
  const [loading, setLoading] = createSignal(false);

  const loadSearchPage = async (
    nextQuery: string,
    startIndex: number,
    replace = false,
  ) => {
    if (loading()) return;
    setLoading(true);
    const result = await fetchVideoSearchPage(nextQuery, startIndex);
    setState((current) => {
      if (!replace && current?.kind === 'ready' && result.kind === 'ready') {
        return {
          kind: 'ready',
          page: result.page,
          items: [...current.items, ...result.items],
        };
      }
      return result;
    });
    setLoading(false);
  };
  const submitSearch = () => {
    const nextQuery = query().trim();
    setSubmittedQuery(nextQuery);
    setState(null);
    void loadSearchPage(nextQuery, 0, true);
  };
  const readyState = () => {
    const current = state();
    return current?.kind === 'ready' ? current : null;
  };
  const statusTitle = () => {
    const current = state();
    if (!current) return loading() ? 'Searching Library' : null;
    if (current.kind === 'empty') return 'No video search results';
    if (current.kind === 'error') return 'Could not search Library';
    if (current.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return null;
  };
  const statusDescription = () => {
    const current = state();
    if (!current) return 'JMSR is searching Movies, Shows, and Episodes.';
    if (current.kind === 'empty') {
      return `Jellyfin returned no video results for "${current.page.query}".`;
    }
    if (current.kind === 'error') return current.message;
    if (current.kind === 'disconnected') {
      return 'Reconnect Jellyfin to search video libraries. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is searching Movies, Shows, and Episodes.';
  };
  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  return (
    <div class="space-y-4">
      <form
        class="card-filled flex flex-col gap-3 sm:flex-row sm:items-end"
        aria-label="Library search"
        onSubmit={(event) => {
          event.preventDefault();
          submitSearch();
        }}
      >
        <label class="min-w-0 flex-1 space-y-2">
          <span class="text-label-small">Search video library</span>
          <input
            class="input-filled w-full"
            value={query()}
            disabled={loading()}
            onInput={(event) => setQuery(event.currentTarget.value)}
          />
        </label>
        <button
          type="submit"
          class="btn-primary rounded-full"
          disabled={loading()}
        >
          <span>{loading() ? 'Searching' : 'Search'}</span>
        </button>
      </form>

      <Show when={state() !== null || loading()}>
        <Show
          when={state()?.kind === 'ready'}
          fallback={
            <div class="space-y-3">
              <LibraryStatusPanel
                title={statusTitle() ?? 'Searching Library'}
                description={statusDescription()}
              />
              <Show
                when={
                  submittedQuery() &&
                  (state()?.kind === 'error' ||
                    state()?.kind === 'disconnected')
                }
              >
                <button
                  type="button"
                  class="btn-secondary rounded-full"
                  disabled={loading()}
                  onClick={() => void loadSearchPage(submittedQuery(), 0, true)}
                >
                  <RefreshCw class="h-4 w-4" />
                  <span>Retry Search</span>
                </button>
              </Show>
            </div>
          }
        >
          <section class="space-y-4" aria-labelledby="library-search-results">
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <h2 id="library-search-results" class="text-title-large">
                Search results
              </h2>
              <p class="text-body-small">
                {readyState()?.items.length ?? 0} of{' '}
                {readyState()?.page.totalRecordCount ?? 0} for "
                {submittedQuery()}"
              </p>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              <For each={readyState()?.items ?? []}>
                {(item) => <VideoLibraryCard item={item} />}
              </For>
            </div>
            <Show when={readyState()?.page.hasMore}>
              <div class="flex justify-center pt-2">
                <button
                  type="button"
                  class="btn-secondary rounded-full"
                  disabled={loading()}
                  onClick={() =>
                    void loadSearchPage(submittedQuery(), loadMoreStartIndex())
                  }
                >
                  <RefreshCw class="h-4 w-4" />
                  <span>
                    {loading() ? 'Loading more' : 'Load more results'}
                  </span>
                </button>
              </div>
            </Show>
          </section>
        </Show>
      </Show>
    </div>
  );
}

function LibraryStatusPanel(props: { title: string; description?: string }) {
  return (
    <section
      class="card-elevated space-y-5"
      aria-labelledby="video-home-status-title"
    >
      <div class="flex items-start gap-4">
        <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl border border-tertiary/30 bg-tertiary-container/25 text-tertiary">
          <Clapperboard class="h-6 w-6" />
        </div>
        <div class="space-y-2">
          <h2 id="video-home-status-title" class="text-headline-small">
            {props.title}
          </h2>
          <p class="text-body-medium">
            {props.description ??
              'JMSR is checking the current Jellyfin session before loading Library data.'}
          </p>
        </div>
      </div>
    </section>
  );
}

function VideoHomeRow(props: {
  id: string;
  title: string;
  items: VideoHomeItem[];
}) {
  return (
    <Show when={props.items.length > 0}>
      <section class="space-y-3" aria-labelledby={`row-${props.id}`}>
        <h2 id={`row-${props.id}`} class="text-title-large">
          {props.title}
        </h2>
        <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <For each={props.items}>
            {(item) => <VideoHomeCard item={item} />}
          </For>
        </div>
      </section>
    </Show>
  );
}

function VideoHomeCard(props: { item: VideoHomeItem }) {
  const episodeLabel = () => {
    if (!props.item.seriesName) return props.item.itemType;
    const number =
      props.item.seasonNumber && props.item.episodeNumber
        ? `S${props.item.seasonNumber.toString().padStart(2, '0')}E${props.item.episodeNumber.toString().padStart(2, '0')}`
        : props.item.itemType;
    return `${props.item.seriesName} · ${number}`;
  };

  return (
    <a
      href={`/library/items/${props.item.id}`}
      class="card-filled group block min-h-56 overflow-hidden p-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70"
    >
      <div class="aspect-video border-b border-outline-variant bg-surface-container-lowest/60">
        <Show
          when={props.item.artworkUrl}
          fallback={
            <div class="flex h-full items-center justify-center px-4 text-center text-label-small text-on-surface-variant">
              No artwork
            </div>
          }
        >
          {(artworkUrl) => (
            <img
              src={artworkUrl()}
              alt={`${props.item.name} artwork`}
              class="h-full w-full object-cover"
              loading="lazy"
            />
          )}
        </Show>
      </div>
      <div class="space-y-2 p-4">
        <p class="line-clamp-2 text-title-medium">{props.item.name}</p>
        <p class="text-body-small">{episodeLabel()}</p>
        <Show when={props.item.resumePositionSeconds !== null}>
          <p class="text-label-small text-secondary">
            Resume at {Math.floor(props.item.resumePositionSeconds ?? 0)}s
          </p>
        </Show>
      </div>
    </a>
  );
}

function LibraryShortcutRow(props: { shortcuts: VideoLibraryShortcut[] }) {
  return (
    <Show when={props.shortcuts.length > 0}>
      <section class="space-y-3" aria-labelledby="library-shortcuts">
        <h2 id="library-shortcuts" class="text-title-large">
          Video Libraries
        </h2>
        <div class="grid gap-3 sm:grid-cols-2">
          <For each={props.shortcuts}>
            {(shortcut) => (
              <a
                href={`/library/${shortcut.collectionType}/${shortcut.id}`}
                class="card-filled flex items-center justify-between gap-4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70"
              >
                <div>
                  <p class="text-title-medium">{shortcut.name}</p>
                  <p class="text-body-small">
                    {shortcut.collectionType === 'tvshows' ? 'Shows' : 'Movies'}{' '}
                    {shortcut.itemCount !== null
                      ? `· ${shortcut.itemCount} items`
                      : ''}
                  </p>
                </div>
                <Library class="h-5 w-5 shrink-0 text-secondary" />
              </a>
            )}
          </For>
        </div>
      </section>
    </Show>
  );
}

function libraryTitle(collectionType: VideoLibraryKind) {
  return collectionType === 'tvshows' ? 'Shows' : 'Movies';
}

function playedFilterLabel(filter: VideoLibraryPlayedFilter) {
  switch (filter) {
    case 'played':
      return 'Played';
    case 'unplayed':
      return 'Unplayed';
    default:
      return 'All';
  }
}

const sortCollection = createListCollection({
  items: [
    { value: 'title', label: 'Title' },
    { value: 'recentlyAdded', label: 'Recently added' },
    { value: 'releaseDate', label: 'Release date' },
  ],
});

export function LibraryBrowseView(props: {
  collectionType: VideoLibraryKind;
  libraryId: string;
}) {
  const [state, setState] = createSignal<LibraryBrowseState | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [sort, setSort] = createSignal<VideoLibrarySort>('title');
  const [playedFilter, setPlayedFilter] =
    createSignal<VideoLibraryPlayedFilter>('all');
  const [favoritesOnly, setFavoritesOnly] = createSignal(false);

  const loadPage = async (startIndex: number, replace = false) => {
    if (loading()) return;
    setLoading(true);
    const result = await fetchVideoLibraryPage(
      props.collectionType,
      props.libraryId,
      startIndex,
      sort(),
      playedFilter(),
      favoritesOnly(),
    );
    setState((current) => {
      if (!replace && current?.kind === 'ready' && result.kind === 'ready') {
        return {
          kind: 'ready',
          page: result.page,
          items: [...current.items, ...result.items],
        };
      }
      return result;
    });
    setLoading(false);
  };
  const reloadFromFirstPage = () => {
    setState(null);
    void loadPage(0, true);
  };

  onMount(() => {
    void loadPage(0, true);
  });

  const readyState = () => {
    const current = state();
    return current?.kind === 'ready' ? current : null;
  };
  const statusTitle = () => {
    const current = state();
    if (!current) return `Loading ${libraryTitle(props.collectionType)}`;
    if (current.kind === 'empty') {
      return `${libraryTitle(props.collectionType)} has no results`;
    }
    if (current.kind === 'error') return 'Could not load Library page';
    if (current.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return `Loading ${libraryTitle(props.collectionType)}`;
  };
  const statusDescription = () => {
    const current = state();
    if (current?.kind === 'empty') {
      return 'Jellyfin returned an empty server page for this video library.';
    }
    if (current?.kind === 'error') return current.message;
    if (current?.kind === 'disconnected') {
      return 'Reconnect Jellyfin to browse video libraries. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is loading a server-paged video library result set.';
  };
  const loadMoreStartIndex = () => {
    const current = readyState();
    return current ? current.page.startIndex + current.page.limit : 0;
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p class="text-label-small text-secondary">Library Browser</p>
          <h1 class="text-headline-large">
            {libraryTitle(props.collectionType)}
          </h1>
          <p class="mt-2 max-w-2xl text-body-large">
            Server-paged video results from Jellyfin.
          </p>
        </div>
        <a href="/library" class="btn-outlined rounded-full">
          <Library class="h-4 w-4" />
          <span>Video Home</span>
        </a>
      </div>

      <section class="card-filled space-y-4" aria-label="Library controls">
        <div class="grid gap-4 lg:grid-cols-[minmax(180px,240px)_1fr_auto] lg:items-end">
          <Select.Root
            collection={sortCollection}
            closeOnSelect
            disabled={loading()}
            value={[sort()]}
            onValueChange={(details) => {
              if (details.value.length > 0) {
                setSort(details.value[0] as VideoLibrarySort);
                reloadFromFirstPage();
              }
            }}
            class="w-full"
          >
            <Select.Label class="mb-1.5 block text-label-small">
              Sort
            </Select.Label>
            <Select.Control class="select-filled flex w-full items-center">
              <Select.Trigger class="flex h-14 w-full items-center justify-between gap-2 rounded-2xl border border-outline-variant/80 bg-surface-container-highest/30 px-4 text-on-surface outline-none transition-all duration-200 hover:border-secondary/50 focus:border-secondary focus:ring-4 focus:ring-secondary/15 disabled:cursor-not-allowed disabled:opacity-50">
                <Select.ValueText
                  placeholder="Select sort…"
                  class="font-medium text-body-medium text-on-surface"
                />
                <Select.Indicator>
                  <ChevronDown class="h-4 w-4 text-on-surface-variant/70" />
                </Select.Indicator>
              </Select.Trigger>
            </Select.Control>
            <Select.Positioner>
              <Select.Content class="mt-2 rounded-2xl border border-outline-variant bg-surface-container-lowest p-2 shadow-2xl backdrop-blur-md max-h-60 overflow-y-auto z-50">
                <For each={sortCollection.items}>
                  {(item) => (
                    <Select.Item
                      item={item}
                      class="flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-body-medium text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface transition-colors"
                    >
                      <Select.ItemText class="font-medium">
                        {item.label}
                      </Select.ItemText>
                    </Select.Item>
                  )}
                </For>
              </Select.Content>
            </Select.Positioner>
            <Select.HiddenSelect />
          </Select.Root>
          <fieldset class="space-y-2" aria-label="Played filter">
            <legend class="text-label-small">Played filter</legend>
            <div class="flex flex-wrap gap-2">
              <For
                each={
                  ['all', 'played', 'unplayed'] as VideoLibraryPlayedFilter[]
                }
              >
                {(filter) => (
                  <button
                    type="button"
                    class={`btn-outlined rounded-full ${
                      playedFilter() === filter
                        ? 'border-secondary bg-secondary-container/45 text-on-secondary-container'
                        : ''
                    }`}
                    aria-pressed={playedFilter() === filter}
                    disabled={loading()}
                    onClick={() => {
                      setPlayedFilter(filter);
                      reloadFromFirstPage();
                    }}
                  >
                    {playedFilterLabel(filter)}
                  </button>
                )}
              </For>
            </div>
          </fieldset>
          <label class="inline-flex min-h-11 items-center gap-3 rounded-xl border border-outline-variant px-3 text-label-large">
            <input
              type="checkbox"
              class="h-4 w-4 accent-secondary"
              checked={favoritesOnly()}
              disabled={loading()}
              onChange={(event) => {
                setFavoritesOnly(event.currentTarget.checked);
                reloadFromFirstPage();
              }}
            />
            <span>Favorites</span>
          </label>
        </div>
      </section>

      <Show
        when={state()?.kind === 'ready'}
        fallback={
          <LibraryStatusPanel
            title={statusTitle()}
            description={statusDescription()}
          />
        }
      >
        <section class="space-y-4" aria-labelledby="library-browse-title">
          <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <h2 id="library-browse-title" class="text-title-large">
              {libraryTitle(props.collectionType)}
            </h2>
            <p class="text-body-small">
              {readyState()?.items.length ?? 0} of{' '}
              {readyState()?.page.totalRecordCount ?? 0}
            </p>
          </div>
          <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
            <For each={readyState()?.items ?? []}>
              {(item) => (
                <VideoLibraryCard
                  item={item}
                  collectionType={props.collectionType}
                />
              )}
            </For>
          </div>
          <Show when={readyState()?.page.hasMore}>
            <div class="flex justify-center pt-2">
              <button
                type="button"
                class="btn-secondary rounded-full"
                disabled={loading()}
                onClick={() => void loadPage(loadMoreStartIndex())}
              >
                <RefreshCw class="h-4 w-4" />
                <span>{loading() ? 'Loading more' : 'Load more'}</span>
              </button>
            </div>
          </Show>
        </section>
      </Show>
    </div>
  );
}

function VideoLibraryCard(props: {
  item: VideoLibraryItem;
  collectionType?: VideoLibraryKind;
}) {
  const Icon =
    props.collectionType === 'tvshows' || props.item.itemType === 'Series'
      ? Tv
      : Film;
  const href = () =>
    props.item.itemType === 'Series'
      ? `/library/shows/${props.item.id}`
      : `/library/items/${props.item.id}`;
  const subtitle = () => {
    const year = props.item.productionYear
      ? props.item.productionYear.toString()
      : props.item.itemType;
    const state = props.item.played ? 'Played' : 'Unplayed';
    return `${year} · ${state}`;
  };

  return (
    <a
      href={href()}
      class="card-filled group block min-h-56 overflow-hidden p-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70"
    >
      <div class="aspect-video border-b border-outline-variant bg-surface-container-lowest/60">
        <Show
          when={props.item.artworkUrl}
          fallback={
            <div class="flex h-full flex-col items-center justify-center gap-2 px-4 text-center text-label-small text-on-surface-variant">
              <Icon class="h-5 w-5" />
              <span>No artwork</span>
            </div>
          }
        >
          {(artworkUrl) => (
            <img
              src={artworkUrl()}
              alt={`${props.item.name} artwork`}
              class="h-full w-full object-cover"
              loading="lazy"
            />
          )}
        </Show>
      </div>
      <div class="space-y-2 p-4">
        <p class="line-clamp-2 text-title-medium">{props.item.name}</p>
        <p class="text-body-small">{subtitle()}</p>
        <Show when={props.item.favorite}>
          <p class="text-label-small text-secondary">Favorite</p>
        </Show>
      </div>
    </a>
  );
}

function formatRuntime(seconds: number | null) {
  if (seconds === null) return null;
  const totalMinutes = Math.round(seconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

function detailSubtitle(detail: VideoItemDetail) {
  if (detail.itemType === 'Episode' && detail.seriesName) {
    const episode =
      detail.seasonNumber !== null && detail.episodeNumber !== null
        ? `S${detail.seasonNumber.toString().padStart(2, '0')}E${detail.episodeNumber.toString().padStart(2, '0')}`
        : 'Episode';
    return `${detail.seriesName} · ${episode}`;
  }
  return detail.productionYear?.toString() ?? detail.itemType;
}

function showSubtitle(detail: VideoShowDetail) {
  return detail.productionYear?.toString() ?? 'Series';
}

function seasonLabel(season: VideoSeason) {
  return season.seasonNumber !== null
    ? `Season ${season.seasonNumber}`
    : season.name;
}

function UserDataControls(props: {
  itemId: string;
  played: boolean;
  favorite: boolean;
  subject: string;
  onSuccess: () => void;
}) {
  const [busy, setBusy] = createSignal<VideoUserDataAction | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const runAction = async (action: VideoUserDataAction) => {
    if (busy()) return;

    setBusy(action);
    setError(null);
    const message = await updateLibraryUserData({
      itemId: props.itemId,
      action,
    });
    setError(message);
    setBusy(null);
    if (!message) props.onSuccess();
  };
  const favoriteAction = () => (props.favorite ? 'unfavorite' : 'favorite');
  const playedAction = () => (props.played ? 'markUnplayed' : 'markPlayed');

  return (
    <div class="space-y-2">
      <div class="flex flex-wrap gap-3">
        <button
          type="button"
          class="btn-secondary rounded-full"
          disabled={busy() !== null}
          onClick={() => void runAction(favoriteAction())}
        >
          {busy() === favoriteAction()
            ? 'Updating'
            : props.favorite
              ? `Remove ${props.subject} favorite`
              : `Favorite ${props.subject}`}
        </button>
        <button
          type="button"
          class="btn-secondary rounded-full"
          disabled={busy() !== null}
          onClick={() => void runAction(playedAction())}
        >
          {busy() === playedAction()
            ? 'Updating'
            : props.played
              ? `Mark ${props.subject} unplayed`
              : `Mark ${props.subject} played`}
        </button>
      </div>
      <Show when={error()}>
        {(message) => <p class="text-body-small text-error">{message()}</p>}
      </Show>
    </div>
  );
}

export function LibraryItemDetailView(props: { itemId: string }) {
  const [state, { refetch }] = createResource(() =>
    fetchVideoItemDetail(props.itemId),
  );
  const [playBusy, setPlayBusy] = createSignal<VideoLibraryPlayMode | null>(
    null,
  );
  const [playError, setPlayError] = createSignal<string | null>(null);
  const detail = () => {
    const current = state();
    return current?.kind === 'ready' ? current.detail : null;
  };
  const playItem = async (mode: VideoLibraryPlayMode) => {
    const item = detail();
    if (!item || playBusy()) return;

    setPlayBusy(mode);
    setPlayError(null);
    const message = await startLibraryPlayback({
      itemId: item.id,
      mode,
      startPositionSeconds: mode === 'resume' ? item.resumePositionSeconds : 0,
    });
    setPlayError(message);
    setPlayBusy(null);
  };
  const statusTitle = () => {
    const current = state();
    if (current?.kind === 'error') return 'Could not load item detail';
    if (current?.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return 'Loading item detail';
  };
  const statusDescription = () => {
    const current = state();
    if (current?.kind === 'error') return current.message;
    if (current?.kind === 'disconnected') {
      return 'Reconnect Jellyfin to inspect video details. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is loading Movie or Episode detail data from Jellyfin.';
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <a href="/library" class="btn-outlined rounded-full">
          <Library class="h-4 w-4" />
          <span>Video Home</span>
        </a>
        <button
          type="button"
          class="btn-outlined rounded-full"
          disabled={state.loading}
          onClick={() => void refetch()}
        >
          <RefreshCw class="h-4 w-4" />
          <span>Retry Detail</span>
        </button>
      </div>

      <Show
        when={detail()}
        fallback={
          <LibraryStatusPanel
            title={statusTitle()}
            description={statusDescription()}
          />
        }
      >
        {(item) => (
          <article class="grid gap-6 lg:grid-cols-[minmax(240px,360px)_1fr]">
            <div class="card-filled overflow-hidden p-0">
              <div class="aspect-[2/3] bg-surface-container-lowest/60">
                <Show
                  when={item().artworkUrl}
                  fallback={
                    <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-on-surface-variant">
                      <Film class="h-8 w-8" />
                      <p class="text-title-medium">{item().name}</p>
                      <p class="text-label-small">No artwork</p>
                    </div>
                  }
                >
                  {(artworkUrl) => (
                    <img
                      src={artworkUrl()}
                      alt={`${item().name} artwork`}
                      class="h-full w-full object-cover"
                    />
                  )}
                </Show>
              </div>
            </div>
            <div class="space-y-5">
              <div>
                <p class="text-label-small text-secondary">{item().itemType}</p>
                <h1 class="text-headline-large">{item().name}</h1>
                <p class="mt-2 text-body-large">{detailSubtitle(item())}</p>
              </div>
              <div class="flex flex-wrap gap-2">
                <StatusBadge variant={item().played ? 'success' : 'neutral'}>
                  {item().played ? 'Played' : 'Unplayed'}
                </StatusBadge>
                <StatusBadge variant={item().favorite ? 'success' : 'neutral'}>
                  {item().favorite ? 'Favorite' : 'Not favorite'}
                </StatusBadge>
                <Show when={formatRuntime(item().runtimeSeconds)}>
                  {(runtime) => (
                    <StatusBadge variant="neutral">{runtime()}</StatusBadge>
                  )}
                </Show>
              </div>
              <UserDataControls
                itemId={item().id}
                played={item().played}
                favorite={item().favorite}
                subject={item().itemType.toLowerCase()}
                onSuccess={() => void refetch()}
              />
              <Show when={item().overview}>
                {(overview) => <p class="text-body-medium">{overview()}</p>}
              </Show>
              <Show when={item().genres.length > 0}>
                <div class="flex flex-wrap gap-2">
                  <For each={item().genres}>
                    {(genre) => (
                      <span class="rounded-full border border-outline-variant px-3 py-1 text-label-small">
                        {genre}
                      </span>
                    )}
                  </For>
                </div>
              </Show>
              <Show when={item().resumePositionSeconds !== null}>
                <p class="text-body-small text-secondary">
                  Resume at {Math.floor(item().resumePositionSeconds ?? 0)}s
                  {item().playedPercentage !== null
                    ? ` · ${Math.round(item().playedPercentage ?? 0)}% watched`
                    : ''}
                </p>
              </Show>
              <div class="flex flex-wrap gap-3">
                <Show
                  when={item().canResume}
                  fallback={
                    <button
                      type="button"
                      class="btn-primary rounded-full"
                      disabled={playBusy() !== null}
                      onClick={() => void playItem('start')}
                    >
                      {playBusy() === 'start' ? 'Starting' : 'Play'}
                    </button>
                  }
                >
                  <button
                    type="button"
                    class="btn-primary rounded-full"
                    disabled={playBusy() !== null}
                    onClick={() => void playItem('resume')}
                  >
                    {playBusy() === 'resume' ? 'Starting' : 'Resume'}
                  </button>
                  <button
                    type="button"
                    class="btn-secondary rounded-full"
                    disabled={playBusy() !== null}
                    onClick={() => void playItem('start')}
                  >
                    {playBusy() === 'start'
                      ? 'Starting'
                      : 'Play from beginning'}
                  </button>
                </Show>
              </div>
              <Show when={playError()}>
                {(message) => (
                  <p class="text-body-small text-error">{message()}</p>
                )}
              </Show>
            </div>
          </article>
        )}
      </Show>
    </div>
  );
}

export function LibraryShowDetailView(props: { seriesId: string }) {
  const [state, { refetch }] = createResource(() =>
    fetchVideoShowDetail(props.seriesId),
  );
  const [selectedSeason, setSelectedSeason] = createSignal<VideoSeason | null>(
    null,
  );
  const [episodes, setEpisodes] = createSignal<SeasonEpisodesState | null>(
    null,
  );
  const [episodesLoading, setEpisodesLoading] = createSignal(false);
  const [playBusy, setPlayBusy] = createSignal(false);
  const [playError, setPlayError] = createSignal<string | null>(null);
  const detail = () => {
    const current = state();
    return current?.kind === 'ready' ? current.detail : null;
  };
  const seasonEpisodes = () => {
    const current = episodes();
    return current?.kind === 'ready' ? current.page.episodes : [];
  };
  const loadEpisodes = async (season: VideoSeason) => {
    if (episodesLoading()) return;
    setSelectedSeason(season);
    setEpisodes(null);
    setEpisodesLoading(true);
    const result = await fetchSeasonEpisodes({
      seriesId: props.seriesId,
      seasonId: season.id,
      seasonNumber: season.seasonNumber,
    });
    setEpisodes(result);
    setEpisodesLoading(false);
  };
  const playShow = async () => {
    const show = detail();
    if (!show || playBusy()) return;

    setPlayBusy(true);
    setPlayError(null);
    const message = await startLibraryPlayback({
      itemId: show.id,
      mode: 'show',
      startPositionSeconds: null,
    });
    setPlayError(message);
    setPlayBusy(false);
  };
  const statusTitle = () => {
    const current = state();
    if (current?.kind === 'error') return 'Could not load show detail';
    if (current?.kind === 'disconnected') {
      return 'Library requires a live Jellyfin connection';
    }
    return 'Loading show detail';
  };
  const statusDescription = () => {
    const current = state();
    if (current?.kind === 'error') return current.message;
    if (current?.kind === 'disconnected') {
      return 'Reconnect Jellyfin to inspect show details. Saved Sessions remain available, but Library data is not cached offline.';
    }
    return 'JMSR is loading Show detail, seasons, and Jellyfin next-up data.';
  };
  const episodesStatusTitle = () => {
    const current = episodes();
    if (episodesLoading()) return 'Loading season episodes';
    if (current?.kind === 'empty') return 'Season has no episodes';
    if (current?.kind === 'error') return 'Could not load season episodes';
    return 'Choose a season';
  };
  const episodesStatusDescription = () => {
    const current = episodes();
    if (episodesLoading()) {
      return 'JMSR is loading exact Episode cards for the selected Season.';
    }
    if (current?.kind === 'empty') {
      return 'Jellyfin returned no Episodes for the selected Season.';
    }
    if (current?.kind === 'error') return current.message;
    return 'Season buttons keep manual episode selection available alongside Jellyfin next-up resolution.';
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <a href="/library" class="btn-outlined rounded-full">
          <Library class="h-4 w-4" />
          <span>Video Home</span>
        </a>
        <button
          type="button"
          class="btn-outlined rounded-full"
          disabled={state.loading}
          onClick={() => void refetch()}
        >
          <RefreshCw class="h-4 w-4" />
          <span>Retry Show</span>
        </button>
      </div>

      <Show
        when={detail()}
        fallback={
          <LibraryStatusPanel
            title={statusTitle()}
            description={statusDescription()}
          />
        }
      >
        {(show) => (
          <div class="space-y-6">
            <article class="grid gap-6 lg:grid-cols-[minmax(240px,360px)_1fr]">
              <div class="card-filled overflow-hidden p-0">
                <div class="aspect-[2/3] bg-surface-container-lowest/60">
                  <Show
                    when={show().artworkUrl}
                    fallback={
                      <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-on-surface-variant">
                        <Tv class="h-8 w-8" />
                        <p class="text-title-medium">{show().name}</p>
                        <p class="text-label-small">No artwork</p>
                      </div>
                    }
                  >
                    {(artworkUrl) => (
                      <img
                        src={artworkUrl()}
                        alt={`${show().name} artwork`}
                        class="h-full w-full object-cover"
                      />
                    )}
                  </Show>
                </div>
              </div>
              <div class="space-y-5">
                <div>
                  <p class="text-label-small text-secondary">Series</p>
                  <h1 class="text-headline-large">{show().name}</h1>
                  <p class="mt-2 text-body-large">{showSubtitle(show())}</p>
                </div>
                <div class="flex flex-wrap gap-2">
                  <StatusBadge variant={show().played ? 'success' : 'neutral'}>
                    {show().played ? 'Played' : 'Unplayed'}
                  </StatusBadge>
                  <StatusBadge
                    variant={show().favorite ? 'success' : 'neutral'}
                  >
                    {show().favorite ? 'Favorite' : 'Not favorite'}
                  </StatusBadge>
                </div>
                <UserDataControls
                  itemId={show().id}
                  played={show().played}
                  favorite={show().favorite}
                  subject="show"
                  onSuccess={() => void refetch()}
                />
                <Show when={show().overview}>
                  {(overview) => <p class="text-body-medium">{overview()}</p>}
                </Show>
                <Show when={show().genres.length > 0}>
                  <div class="flex flex-wrap gap-2">
                    <For each={show().genres}>
                      {(genre) => (
                        <span class="rounded-full border border-outline-variant px-3 py-1 text-label-small">
                          {genre}
                        </span>
                      )}
                    </For>
                  </div>
                </Show>
                <Show
                  when={show().nextEpisode}
                  fallback={
                    <button
                      type="button"
                      class="btn-primary rounded-full"
                      disabled
                    >
                      Play unavailable
                    </button>
                  }
                >
                  {(nextEpisode) => (
                    <div class="flex flex-col gap-3 sm:flex-row sm:items-center">
                      <button
                        type="button"
                        class="btn-primary rounded-full"
                        disabled={playBusy()}
                        onClick={() => void playShow()}
                      >
                        {playBusy() ? 'Starting' : 'Play'}
                      </button>
                      <a
                        href={`/library/items/${nextEpisode().id}`}
                        class="text-body-small text-secondary underline-offset-4 hover:underline"
                      >
                        Next: {nextEpisode().name}
                      </a>
                    </div>
                  )}
                </Show>
                <Show when={playError()}>
                  {(message) => (
                    <p class="text-body-small text-error">{message()}</p>
                  )}
                </Show>
              </div>
            </article>

            <section class="space-y-4" aria-labelledby="show-seasons-title">
              <div class="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
                <div>
                  <p class="text-label-small text-secondary">Manual Browse</p>
                  <h2 id="show-seasons-title" class="text-title-large">
                    Seasons and Episodes
                  </h2>
                </div>
                <p class="text-body-small">
                  {show().seasons.length} seasons available
                </p>
              </div>

              <Show
                when={show().seasons.length > 0}
                fallback={
                  <LibraryStatusPanel
                    title="No seasons available"
                    description="Jellyfin returned no seasons for this show."
                  />
                }
              >
                <ul
                  class="flex gap-2 overflow-x-auto rounded-2xl border border-outline-variant bg-surface-container-low/70 p-2"
                  aria-label="Show seasons"
                >
                  <For each={show().seasons}>
                    {(season) => (
                      <li class="shrink-0">
                        <button
                          type="button"
                          class={`btn-outlined rounded-full ${
                            selectedSeason()?.id === season.id
                              ? 'border-secondary bg-secondary-container/45 text-on-secondary-container'
                              : ''
                          }`}
                          aria-pressed={selectedSeason()?.id === season.id}
                          disabled={episodesLoading()}
                          onClick={() => void loadEpisodes(season)}
                        >
                          <span>{seasonLabel(season)}</span>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>

                <Show when={selectedSeason()}>
                  {(season) => (
                    <UserDataControls
                      itemId={season().id}
                      played={season().played}
                      favorite={season().favorite}
                      subject={seasonLabel(season()).toLowerCase()}
                      onSuccess={() => void refetch()}
                    />
                  )}
                </Show>

                <Show
                  when={episodes()?.kind === 'ready'}
                  fallback={
                    <LibraryStatusPanel
                      title={episodesStatusTitle()}
                      description={episodesStatusDescription()}
                    />
                  }
                >
                  <section
                    class="space-y-4"
                    aria-labelledby="season-episodes-title"
                  >
                    <h3 id="season-episodes-title" class="text-title-medium">
                      {selectedSeason()
                        ? `${selectedSeason()?.name} Episodes`
                        : 'Episodes'}
                    </h3>
                    <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                      <For each={seasonEpisodes()}>
                        {(episode) => <VideoLibraryCard item={episode} />}
                      </For>
                    </div>
                  </section>
                </Show>
              </Show>
            </section>
          </div>
        )}
      </Show>
    </div>
  );
}

export function DiagnosticsArea() {
  return (
    <section
      class="card-elevated space-y-5"
      aria-labelledby="diagnostics-title"
    >
      <div>
        <p class="text-label-small text-secondary">Runtime</p>
        <h1 id="diagnostics-title" class="text-headline-large">
          Diagnostics
        </h1>
      </div>
      <DiagnosticsPanel />
    </section>
  );
}

export default function AuthenticatedShell() {
  const [connection] = createResource(() => commands.jellyfinGetState());
  const nowPlayingMatch = useMatch({
    from: '/authenticated/now-playing',
    shouldThrow: false,
  });
  const showCompactNowPlaying = () => nowPlayingMatch() === undefined;

  return (
    <div class="console-shell">
      <div class="mx-auto flex w-full max-w-7xl flex-col gap-5">
        <ShellHeader connection={connection()} />
        <div class="flex flex-col gap-6 min-w-0">
          <Show when={showCompactNowPlaying()}>
            <CompactNowPlayingSummary />
          </Show>
          <main class="min-w-0 animate-fade-in">
            <Outlet />
          </main>
        </div>
      </div>
    </div>
  );
}
