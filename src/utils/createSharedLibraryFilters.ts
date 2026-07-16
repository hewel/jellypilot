import { load } from '@tauri-apps/plugin-store';
import { Effect, Exit, Match, Option } from 'effect';
import type { Accessor, Setter } from 'solid-js';
import { createEffect, createSignal } from 'solid-js';

import type { VideoLibraryPlayedFilter, VideoLibrarySort } from '../bindings';

export type LibrarySortDirection = 'asc' | 'desc';

const PREFERENCES_STORE_FILE = 'preferences.json';
const LIBRARY_FILTERS_STORE_KEY = 'library_filters';
const LEGACY_STORAGE_KEY = 'jellypilot_library_filters';
const DEFAULT_SORT: VideoLibrarySort = 'title';
const DEFAULT_PLAYED_FILTER: VideoLibraryPlayedFilter = 'all';
const DEFAULT_FAVORITES_ONLY = false;
const DEFAULT_SORT_DIRECTION: LibrarySortDirection = 'asc';

interface LibraryFilterSnapshot {
  sort: VideoLibrarySort;
  playedFilter: VideoLibraryPlayedFilter;
  favoritesOnly: boolean;
  sortDirection: LibrarySortDirection;
}

const [ready, setReady] = createSignal(false);
const [sort, setSort] = createSignal<VideoLibrarySort>(DEFAULT_SORT);
const [playedFilter, setPlayedFilter] =
  createSignal<VideoLibraryPlayedFilter>(DEFAULT_PLAYED_FILTER);
const [favoritesOnly, setFavoritesOnly] = createSignal(DEFAULT_FAVORITES_ONLY);
const [sortDirection, setSortDirection] =
  createSignal<LibrarySortDirection>(DEFAULT_SORT_DIRECTION);
let writeQueue: Promise<void> | null = null;
let hydratedSnapshot: LibraryFilterSnapshot | null = null;
let hydrateGeneration = 0;
let hydratePromise: Promise<void> | null = null;

export interface SharedLibraryFilters {
  ready: Accessor<boolean>;
  sort: Accessor<VideoLibrarySort>;
  setSort: Setter<VideoLibrarySort>;
  playedFilter: Accessor<VideoLibraryPlayedFilter>;
  setPlayedFilter: Setter<VideoLibraryPlayedFilter>;
  favoritesOnly: Accessor<boolean>;
  setFavoritesOnly: Setter<boolean>;
  sortDirection: Accessor<LibrarySortDirection>;
  setSortDirection: Setter<LibrarySortDirection>;
}

const parseSort = Match.type<string | undefined>().pipe(
  Match.withReturnType<VideoLibrarySort>(),
  Match.when(Match.is('recentlyAdded', 'releaseDate', 'title'), (value) => value),
  Match.orElse(() => DEFAULT_SORT),
);

const parsePlayedFilter = Match.type<string | undefined>().pipe(
  Match.withReturnType<VideoLibraryPlayedFilter>(),
  Match.when(Match.is('played', 'unplayed', 'all'), (value) => value),
  Match.orElse(() => DEFAULT_PLAYED_FILTER),
);

function parseSortDirection(value: string | undefined): LibrarySortDirection {
  return value === 'desc' ? 'desc' : DEFAULT_SORT_DIRECTION;
}

function defaultSnapshot(): LibraryFilterSnapshot {
  return {
    sort: DEFAULT_SORT,
    playedFilter: DEFAULT_PLAYED_FILTER,
    favoritesOnly: DEFAULT_FAVORITES_ONLY,
    sortDirection: DEFAULT_SORT_DIRECTION,
  };
}

function snapshot(): LibraryFilterSnapshot {
  return {
    sort: sort(),
    playedFilter: playedFilter(),
    favoritesOnly: favoritesOnly(),
    sortDirection: sortDirection(),
  };
}

function snapshotsEqual(left: LibraryFilterSnapshot, right: LibraryFilterSnapshot) {
  return (
    left.sort === right.sort &&
    left.playedFilter === right.playedFilter &&
    left.favoritesOnly === right.favoritesOnly &&
    left.sortDirection === right.sortDirection
  );
}

function applySnapshot(filters: LibraryFilterSnapshot) {
  setSort(filters.sort);
  setPlayedFilter(filters.playedFilter);
  setFavoritesOnly(filters.favoritesOnly);
  setSortDirection(filters.sortDirection);
}

function parseStoreSnapshot(value: unknown): Option.Option<LibraryFilterSnapshot> {
  if (value === null || typeof value !== 'object') {
    return Option.none();
  }
  const obj = value as Record<string, unknown>;
  return Option.some({
    sort: parseSort(typeof obj.sort === 'string' ? obj.sort : undefined),
    playedFilter: parsePlayedFilter(
      typeof obj.playedFilter === 'string' ? obj.playedFilter : undefined,
    ),
    favoritesOnly: obj.favoritesOnly === true,
    sortDirection: parseSortDirection(
      typeof obj.sortDirection === 'string' ? obj.sortDirection : undefined,
    ),
  });
}

function readLegacySnapshot(): Option.Option<LibraryFilterSnapshot> {
  const stored = Effect.runSyncExit(Effect.try(() => localStorage.getItem(LEGACY_STORAGE_KEY)));
  if (Exit.isFailure(stored)) {
    return Option.none();
  }
  return Option.match(Option.fromNullishOr(stored.value), {
    onNone: () => Option.none(),
    onSome: (value) => {
      const [storedSort, storedPlayedFilter, storedFavoritesOnly, storedSortDirection] =
        value.split('|');
      return Option.some({
        sort: parseSort(storedSort),
        playedFilter: parsePlayedFilter(storedPlayedFilter),
        favoritesOnly: storedFavoritesOnly === '1',
        sortDirection: parseSortDirection(storedSortDirection),
      });
    },
  });
}

function removeLegacySnapshot() {
  void Effect.runSyncExit(Effect.try(() => localStorage.removeItem(LEGACY_STORAGE_KEY)));
}

function applyHydratedSnapshot(filters: LibraryFilterSnapshot, generation: number) {
  if (generation !== hydrateGeneration) {
    return false;
  }

  applySnapshot(filters);
  hydratedSnapshot = filters;
  return true;
}

async function hydrateFilters(generation: number) {
  await Effect.runPromiseExit(
    Effect.tryPromise(async () => {
      await writeQueue;
      if (generation !== hydrateGeneration) {
        return;
      }
      const store = await load(PREFERENCES_STORE_FILE, { defaults: {}, autoSave: false });
      const stored = parseStoreSnapshot(await store.get(LIBRARY_FILTERS_STORE_KEY));

      if (Option.isSome(stored)) {
        applyHydratedSnapshot(stored.value, generation);
        return;
      }

      const legacy = readLegacySnapshot();
      const snapshot = Option.match(legacy, {
        onNone: () => defaultSnapshot(),
        onSome: (value) => value,
      });
      if (!applyHydratedSnapshot(snapshot, generation)) {
        return;
      }

      if (Option.isSome(legacy)) {
        await store.set(LIBRARY_FILTERS_STORE_KEY, snapshot);
        await store.save();
        removeLegacySnapshot();
      }
      // Tauri Store can be unavailable in browser-only contexts; in-memory filters still work.
    }),
  );
}

function persistFilters(filters: LibraryFilterSnapshot, generation: number) {
  const write = async () => {
    if (generation !== hydrateGeneration) {
      return;
    }
    // Persistence is best-effort; rendering keeps the current in-memory signals.
    await Effect.runPromiseExit(
      Effect.tryPromise(async () => {
        const store = await load(PREFERENCES_STORE_FILE, { defaults: {}, autoSave: false });
        if (generation !== hydrateGeneration) {
          return;
        }
        await store.set(LIBRARY_FILTERS_STORE_KEY, filters);
        await store.save();
      }),
    );
  };

  writeQueue = (writeQueue ?? Promise.resolve()).then(write, () => undefined);
}

export function resetSharedLibraryFilters() {
  hydrateGeneration += 1;
  hydratePromise = null;
  writeQueue = null;
  hydratedSnapshot = null;
  setReady(false);
  applySnapshot(defaultSnapshot());
}

export function createSharedLibraryFilters(): SharedLibraryFilters {
  const generation = hydrateGeneration;
  let observedInitialReadySnapshot = false;
  if (!ready() && !hydratePromise) {
    hydratePromise = hydrateFilters(generation).finally(() => {
      if (generation === hydrateGeneration) {
        setReady(true);
        hydratePromise = null;
      }
    });
  }

  createEffect(() => {
    if (!ready()) {
      return;
    }
    const filters = snapshot();
    if (!observedInitialReadySnapshot) {
      observedInitialReadySnapshot = true;
      if (hydratedSnapshot && snapshotsEqual(filters, hydratedSnapshot)) {
        hydratedSnapshot = null;
      }
      return;
    }
    if (hydratedSnapshot && snapshotsEqual(filters, hydratedSnapshot)) {
      hydratedSnapshot = null;
      return;
    }
    persistFilters(filters, generation);
  });

  return {
    ready,
    sort,
    setSort,
    playedFilter,
    setPlayedFilter,
    favoritesOnly,
    setFavoritesOnly,
    sortDirection,
    setSortDirection,
  };
}
