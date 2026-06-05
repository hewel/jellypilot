import { Effect, Exit } from 'effect';
import {
  Activity,
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
  type VideoLibraryShortcut,
  type VideoLibrarySort,
  type VideoSearchPage,
} from '../bindings';
import {
  commandFailureMessage,
  runTauriCommand,
  runTauriCommandRaw,
} from '../effects/commands';
import DiagnosticsPanel from './DiagnosticsPanel';
import NowPlayingCard from './NowPlayingCard';
import OperationsConsole from './OperationsConsole';
import { StatusBadge } from './ui';

export type ShellArea = 'library' | 'now-playing' | 'settings' | 'diagnostics';
export type LibraryView =
  | { kind: 'home' }
  | { kind: 'browse'; collectionType: VideoLibraryKind; libraryId: string }
  | { kind: 'detail'; itemId: string };

interface AuthenticatedShellProps {
  activeArea: ShellArea;
  libraryView?: LibraryView;
  onSignedOut: () => void;
}

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

const LIBRARY_BROWSE_PAGE_SIZE = 24;
const LIBRARY_SEARCH_PAGE_SIZE = 24;

const navItems: Array<{
  area: ShellArea;
  href: string;
  label: string;
  Icon: typeof Library;
}> = [
  { area: 'library', href: '/library', label: 'Library', Icon: Library },
  {
    area: 'now-playing',
    href: '/now-playing',
    label: 'Now Playing',
    Icon: MonitorPlay,
  },
  { area: 'settings', href: '/settings', label: 'Settings', Icon: Settings },
  {
    area: 'diagnostics',
    href: '/diagnostics',
    label: 'Diagnostics',
    Icon: Activity,
  },
];

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

function ShellNav(props: { activeArea: ShellArea }) {
  return (
    <nav
      aria-label="JMSR areas"
      class="flex gap-2 overflow-x-auto rounded-2xl border border-outline-variant bg-surface-container-low/70 p-2 shadow-xl backdrop-blur-md lg:flex-col lg:overflow-visible"
    >
      {navItems.map(({ area, href, label, Icon }) => {
        const active = () => props.activeArea === area;
        return (
          <a
            href={href}
            aria-current={active() ? 'page' : undefined}
            class={`inline-flex min-h-11 shrink-0 items-center gap-2 rounded-xl px-3 text-[14px] font-bold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-secondary/70 ${
              active()
                ? 'border border-primary/40 bg-primary-container/65 text-on-primary-container shadow-brand-glow-sm'
                : 'border border-transparent text-on-surface-variant hover:border-outline-variant hover:bg-surface-container-high/60 hover:text-on-surface'
            }`}
          >
            <Icon class="h-4.5 w-4.5" />
            <span>{label}</span>
          </a>
        );
      })}
    </nav>
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

function LibraryLanding() {
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

      <CompactNowPlayingSummary />

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

function LibraryBrowseView(props: {
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

      <CompactNowPlayingSummary />

      <section class="card-filled space-y-4" aria-label="Library controls">
        <div class="grid gap-4 lg:grid-cols-[minmax(180px,240px)_1fr_auto] lg:items-end">
          <label class="space-y-2">
            <span class="text-label-small">Sort</span>
            <select
              class="input-filled w-full"
              value={sort()}
              disabled={loading()}
              onChange={(event) => {
                setSort(event.currentTarget.value as VideoLibrarySort);
                reloadFromFirstPage();
              }}
            >
              <option value="title">Title</option>
              <option value="recentlyAdded">Recently added</option>
              <option value="releaseDate">Release date</option>
            </select>
          </label>
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
  const subtitle = () => {
    const year = props.item.productionYear
      ? props.item.productionYear.toString()
      : props.item.itemType;
    const state = props.item.played ? 'Played' : 'Unplayed';
    return `${year} · ${state}`;
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

function LibraryItemDetailView(props: { itemId: string }) {
  const [state, { refetch }] = createResource(() =>
    fetchVideoItemDetail(props.itemId),
  );
  const detail = () => {
    const current = state();
    return current?.kind === 'ready' ? current.detail : null;
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

      <CompactNowPlayingSummary />

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
                    <button type="button" class="btn-primary rounded-full">
                      Play
                    </button>
                  }
                >
                  <button type="button" class="btn-primary rounded-full">
                    Resume
                  </button>
                  <button type="button" class="btn-secondary rounded-full">
                    Play from beginning
                  </button>
                </Show>
              </div>
            </div>
          </article>
        )}
      </Show>
    </div>
  );
}

function DiagnosticsArea() {
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

export default function AuthenticatedShell(props: AuthenticatedShellProps) {
  const renderArea = () => {
    switch (props.activeArea) {
      case 'library':
        if (props.libraryView?.kind === 'browse') {
          return (
            <LibraryBrowseView
              collectionType={props.libraryView.collectionType}
              libraryId={props.libraryView.libraryId}
            />
          );
        }
        if (props.libraryView?.kind === 'detail') {
          return <LibraryItemDetailView itemId={props.libraryView.itemId} />;
        }
        return <LibraryLanding />;
      case 'now-playing':
        return <NowPlayingCard jellyfinConnected={true} />;
      case 'diagnostics':
        return <DiagnosticsArea />;
      case 'settings':
        return <OperationsConsole onSignedOut={props.onSignedOut} />;
      default:
        return <LibraryLanding />;
    }
  };

  return (
    <div class="console-shell">
      <div class="mx-auto grid w-full max-w-7xl gap-5 lg:grid-cols-[220px_minmax(0,1fr)]">
        <div class="lg:sticky lg:top-6 lg:self-start">
          <ShellNav activeArea={props.activeArea} />
        </div>
        <main class="min-w-0 animate-fade-in">{renderArea()}</main>
      </div>
    </div>
  );
}
