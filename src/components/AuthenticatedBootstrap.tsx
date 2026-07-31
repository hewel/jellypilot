import { createQuery, useQueryClient } from '@tanstack/solid-query';
import { Effect, Exit } from 'effect';
import { type JSX, createContext, createEffect, createMemo, onMount, useContext } from 'solid-js';
import { fetchConnectionState } from '~effects/connection';
import { fetchAppLocalServices } from '~effects/localServices';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  librarySessionSignature,
  queryKeys,
  runExit,
} from '~effects/query';
import type { LibrarySessionKey } from '~effects/query';
import { setImageProxyBase } from '~utils/imageSource';

/**
 * Shell-level bootstrap read model: the single connection-state observer for
 * the authenticated shell plus application-local readiness. Sidebar, Video
 * Home, and Library Browser consume this context instead of mounting their
 * own connection observers.
 */
export interface AuthenticatedBootstrap {
  readonly sessionKey: () => LibrarySessionKey;
  readonly connected: () => boolean;
}

const AuthenticatedBootstrapContext = createContext<AuthenticatedBootstrap>();

export function useAuthenticatedBootstrap(): AuthenticatedBootstrap {
  const context = useContext(AuthenticatedBootstrapContext);
  if (!context) {
    throw new Error('useAuthenticatedBootstrap must be used within AuthenticatedBootstrapProvider');
  }
  return context;
}

export interface AuthenticatedBootstrapProviderProps {
  /** Invoked after a connected Saved Service Profile switch replaces the session identity. */
  readonly onSessionChange?: () => void;
  readonly children: JSX.Element;
}

export function AuthenticatedBootstrapProvider(props: AuthenticatedBootstrapProviderProps) {
  const queryClient = useQueryClient();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const connected = () => isLibrarySessionKeyConnected(sessionKey());

  onMount(() => {
    void Effect.runPromiseExit(fetchAppLocalServices).then((exit) =>
      Exit.match(exit, {
        onFailure: () => setImageProxyBase(null),
        onSuccess: (services) => setImageProxyBase(services?.imageProxyBase ?? null),
      }),
    );
  });

  let mountedSessionSignature: string | null = null;
  createEffect(() => {
    const currentSessionKey = sessionKey();
    const currentSignature = librarySessionSignature(currentSessionKey);
    if (currentSignature === null) {
      return;
    }

    const previousSignature = mountedSessionSignature;
    mountedSessionSignature = currentSignature;
    if (previousSignature === null || previousSignature === currentSignature) {
      return;
    }

    queryClient.removeQueries({ queryKey: queryKeys.libraryRoot });
    props.onSessionChange?.();
  });

  const value: AuthenticatedBootstrap = { sessionKey, connected };
  return (
    <AuthenticatedBootstrapContext.Provider value={value}>
      {props.children}
    </AuthenticatedBootstrapContext.Provider>
  );
}
