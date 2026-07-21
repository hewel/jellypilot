import { load } from '@tauri-apps/plugin-store';
import { Effect } from 'effect';
import type { Accessor, Setter } from 'solid-js';
import { createSignal } from 'solid-js';

const PREFERENCES_STORE_FILE = 'preferences.json';
const SIDEBAR_COLLAPSED_STORE_KEY = 'sidebar_collapsed';
const DEFAULT_COLLAPSED = false;

const [collapsed, setCollapsed] = createSignal(DEFAULT_COLLAPSED);
let writeQueue: Promise<void> | null = null;
let hydrated = false;

export interface SidebarPreferences {
  collapsed: Accessor<boolean>;
  setCollapsed: Setter<boolean>;
}

function parseCollapsed(value: unknown): boolean {
  return value === true;
}

/**
 * Hydrates the shared sidebar preference from the Tauri preferences store.
 * Best-effort: the store can be unavailable in browser-only contexts, so the
 * in-memory default keeps rendering.
 */
export async function hydrateSidebarPreferences(): Promise<void> {
  if (hydrated) {
    return;
  }
  hydrated = true;
  await Effect.runPromiseExit(
    Effect.tryPromise(async () => {
      const store = await load(PREFERENCES_STORE_FILE, { defaults: {}, autoSave: false });
      setCollapsed(parseCollapsed(await store.get(SIDEBAR_COLLAPSED_STORE_KEY)));
    }),
  );
}

function persistCollapsed(value: boolean) {
  const write = async () => {
    // Persistence is best-effort; rendering keeps the current in-memory signal.
    await Effect.runPromiseExit(
      Effect.tryPromise(async () => {
        const store = await load(PREFERENCES_STORE_FILE, { defaults: {}, autoSave: false });
        await store.set(SIDEBAR_COLLAPSED_STORE_KEY, value);
        await store.save();
      }),
    );
  };

  writeQueue = (writeQueue ?? Promise.resolve()).then(write, () => undefined);
}

export function createSidebarPreferences(): SidebarPreferences {
  void hydrateSidebarPreferences();
  return {
    collapsed,
    setCollapsed: (value) => {
      const next = setCollapsed(value);
      persistCollapsed(next);
      return next;
    },
  };
}

export function resetSidebarPreferences() {
  writeQueue = null;
  hydrated = false;
  setCollapsed(DEFAULT_COLLAPSED);
}
