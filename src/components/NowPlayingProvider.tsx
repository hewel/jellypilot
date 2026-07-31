import { createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit } from 'effect';
import { type JSX, createContext, onCleanup, onMount, useContext } from 'solid-js';
import { fetchNowPlayingState, listenNowPlayingChanged } from '~effects/nowPlaying';
import { queryKeys, runExit } from '~effects/query';

import type { NowPlayingState } from '../bindings';

/**
 * Shell-level Now Playing owner: the single query observer and Tauri change
 * listener for the authenticated shell. The Sidebar trigger and the Now
 * Playing controls consume this context so pushed state can never diverge
 * between them, and the listener is disposed exactly once with the shell.
 */
export interface NowPlayingOwner {
  readonly state: () => NowPlayingState | null;
  /** Refetch the shared Now Playing query entry after a successful command. */
  readonly refresh: () => Promise<unknown>;
  /**
   * Register a consumer callback for pushed Now Playing changes (e.g. to
   * clear local drafts or invalidate dependent queries). Registration is
   * cleaned up with the calling consumer's Solid owner.
   */
  readonly onExternalChange: (callback: (state: NowPlayingState) => void) => void;
}

const NowPlayingContext = createContext<NowPlayingOwner>();

export function useNowPlaying(): NowPlayingOwner {
  const context = useContext(NowPlayingContext);
  if (!context) {
    throw new Error('useNowPlaying must be used within NowPlayingProvider');
  }
  return context;
}

export function NowPlayingProvider(props: { children: JSX.Element }) {
  const queryClient = useQueryClient();
  const query = createQuery(() => ({
    queryKey: queryKeys.nowPlayingState,
    queryFn: () => runExit(fetchNowPlayingState),
  }));
  const externalChangeListeners = new Set<(state: NowPlayingState) => void>();

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    listenNowPlayingChanged((state) => {
      queryClient.setQueryData(queryKeys.nowPlayingState, Exit.succeed(state));
      for (const listener of externalChangeListeners) {
        listener(state);
      }
    }).then((unlisten) => {
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

  const state = () => (query.data && Exit.isSuccess(query.data) ? query.data.value : null);
  const refresh = () => query.refetch();
  const onExternalChange = (callback: (state: NowPlayingState) => void) => {
    externalChangeListeners.add(callback);
    onCleanup(() => {
      externalChangeListeners.delete(callback);
    });
  };

  const value: NowPlayingOwner = { state, refresh, onExternalChange };
  return <NowPlayingContext.Provider value={value}>{props.children}</NowPlayingContext.Provider>;
}
