import type { JSX } from 'solid-js';
import { createContext, useContext } from 'solid-js';
import { createStore, type SetStoreFunction } from 'solid-js/store';
import type { IntroSkipperMode } from '../../bindings';

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

export type PlayerBridgeSaveStatus = {
  type: 'saving' | 'saved' | 'error';
  text: string;
};

export interface OperationsConsoleState {
  disconnecting: boolean;
  reconnecting: boolean;
  signingOut: boolean;
  confirmSignOut: boolean;
  detectingMpv: boolean;
  advancedOpen: boolean;
  diagnosticsExpanded: boolean;
  playerBridgeSaveStatus: PlayerBridgeSaveStatus | null;
  introSkipperDraft: IntroSkipperMode | null;
  introSkipperSaving: boolean;
  introSkipperError: string | null;
  selectedSubtitleLanguages: string[];
  subtitleLanguageInput: string;
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

export interface OperationsConsoleActions {
  hydrateFromConfig(
    config: Partial<{
      preferredSubtitleLanguages: string[] | null | undefined;
      introSkipperMode: IntroSkipperMode | undefined;
      mpvArgs: string[] | null | undefined;
    }>,
  ): void;

  showPlayerBridgeStatus(status: PlayerBridgeSaveStatus): void;
  clearPlayerBridgeStatus(): void;
  setAdvancedOpen(open: boolean): void;
  toggleDiagnostics(): void;

  beginIntroSkipperSave(mode: IntroSkipperMode): void;
  finishIntroSkipperSave(): void;
  failIntroSkipperSave(previous: IntroSkipperMode, message: string): void;

  setPreferredSubtitleLanguages(languages: string[]): void;
  setSubtitleLanguageInput(value: string): void;

  beginReconnect(): void;
  finishReconnect(): void;

  beginDisconnect(): void;
  finishDisconnect(): void;

  beginSignOut(): void;
  finishSignOut(): void;
  setSignOutDialogOpen(open: boolean): void;

  beginMpvDetection(): void;
  finishMpvDetection(): void;
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

type StoreValue = readonly [OperationsConsoleState, OperationsConsoleActions];

const OperationsConsoleContext = createContext<StoreValue>();

export function useOperationsConsoleStore(): StoreValue {
  const ctx = useContext(OperationsConsoleContext);
  if (!ctx) {
    throw new Error(
      'useOperationsConsoleStore must be used within OperationsConsoleProvider',
    );
  }
  return ctx;
}

// ---------------------------------------------------------------------------
// Store factory — MUST return a fresh object each call because solid-js/store
// mutates the initial state object in-place via its reactive proxy.
// ---------------------------------------------------------------------------

export function getInitialState(): OperationsConsoleState {
  return {
    disconnecting: false,
    reconnecting: false,
    signingOut: false,
    confirmSignOut: false,
    detectingMpv: false,
    advancedOpen: false,
    diagnosticsExpanded: false,
    playerBridgeSaveStatus: null,
    introSkipperDraft: null,
    introSkipperSaving: false,
    introSkipperError: null,
    selectedSubtitleLanguages: [],
    subtitleLanguageInput: '',
  };
}

function createActions(
  set: SetStoreFunction<OperationsConsoleState>,
): OperationsConsoleActions {
  return {
    hydrateFromConfig(config) {
      set('selectedSubtitleLanguages', config.preferredSubtitleLanguages ?? []);
      if (config.mpvArgs && config.mpvArgs.length > 0) {
        set('advancedOpen', true);
      }
    },

    showPlayerBridgeStatus(status) {
      set('playerBridgeSaveStatus', status);
    },
    clearPlayerBridgeStatus() {
      set('playerBridgeSaveStatus', null);
    },
    setAdvancedOpen(open) {
      set('advancedOpen', open);
    },
    toggleDiagnostics() {
      set('diagnosticsExpanded', (prev) => !prev);
    },

    beginIntroSkipperSave(mode) {
      set('introSkipperDraft', mode);
      set('introSkipperSaving', true);
      set('introSkipperError', null);
    },
    finishIntroSkipperSave() {
      set('introSkipperDraft', null);
      set('introSkipperSaving', false);
    },
    failIntroSkipperSave(previous, message) {
      set('introSkipperDraft', previous);
      set('introSkipperSaving', false);
      set('introSkipperError', message);
    },

    setPreferredSubtitleLanguages(languages) {
      set('selectedSubtitleLanguages', languages);
    },
    setSubtitleLanguageInput(value) {
      set('subtitleLanguageInput', value);
    },

    beginReconnect() {
      set('reconnecting', true);
    },
    finishReconnect() {
      set('reconnecting', false);
    },

    beginDisconnect() {
      set('disconnecting', true);
    },
    finishDisconnect() {
      set('disconnecting', false);
    },

    beginSignOut() {
      set('signingOut', true);
    },
    finishSignOut() {
      set('signingOut', false);
      set('confirmSignOut', false);
    },
    setSignOutDialogOpen(open) {
      set('confirmSignOut', open);
    },

    beginMpvDetection() {
      set('detectingMpv', true);
    },
    finishMpvDetection() {
      set('detectingMpv', false);
    },
  };
}

export function createOperationsConsoleStore(): {
  state: OperationsConsoleState;
  actions: OperationsConsoleActions;
  Provider: (props: { children: JSX.Element }) => JSX.Element;
} {
  const [state, setState] = createStore<OperationsConsoleState>(
    getInitialState(),
  );
  const actions = createActions(setState);

  const Provider = (props: { children: JSX.Element }) => {
    const value = [state, actions] as const;
    return (
      <OperationsConsoleContext.Provider value={value}>
        {props.children}
      </OperationsConsoleContext.Provider>
    );
  };

  return { state, actions, Provider };
}
