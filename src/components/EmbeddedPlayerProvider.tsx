import type {
  EmbeddedPlayerObservation as TauriEmbeddedPlayerObservation,
  EmbeddedPlayerObservationKind,
  EmbeddedPlayerState,
  PlaybackControlCommand,
  WebPlaybackCapabilities,
} from '@bindings';
import { Effect, Exit } from 'effect';
import * as HlsModule from 'hls.js';
import {
  createContext,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  useContext,
} from 'solid-js';
import type { Accessor, JSX } from 'solid-js';
import {
  controlEmbeddedPlayer,
  fetchEmbeddedPlayerState,
  listenEmbeddedPlayerChanged,
  observeEmbeddedPlayer,
  playEmbeddedPlayerInMpv,
  registerEmbeddedPlayerCapabilities,
} from '~effects/embeddedPlayer';
import type { EmbeddedPlayerEffect } from '~effects/embeddedPlayer';

import type {
  EmbeddedPlayerControl,
  EmbeddedPlayerObservation,
  EmbeddedPlayerViewModel,
} from './EmbeddedPlayer';

export interface EmbeddedPlayerOwner {
  readonly player: Accessor<EmbeddedPlayerViewModel | null>;
  readonly settled: Accessor<boolean>;
  readonly control: (command: EmbeddedPlayerControl) => void;
  readonly observe: (observation: EmbeddedPlayerObservation) => void;
  readonly playInMpv: () => void;
}

const EmbeddedPlayerContext = createContext<EmbeddedPlayerOwner>();

export function useEmbeddedPlayer(): EmbeddedPlayerOwner {
  const context = useContext(EmbeddedPlayerContext);
  if (!context) {
    throw new Error('useEmbeddedPlayer must be used within EmbeddedPlayerProvider');
  }
  return context;
}

function hasActiveSession(
  state: EmbeddedPlayerState | null,
): state is EmbeddedPlayerState & { generation: number; sessionId: string; title: string } {
  return (
    state !== null &&
    state.phase !== 'idle' &&
    state.phase !== 'stopped' &&
    state.sessionId !== null &&
    state.generation !== null &&
    state.title !== null
  );
}

function viewModelFromState(state: EmbeddedPlayerState | null): EmbeddedPlayerViewModel | null {
  if (!state || !hasActiveSession(state)) {
    return null;
  }

  const phase = state.phase === 'buffering' ? 'loading' : state.phase;
  return {
    canPlayInMpv: state.canPlayInMpv,
    desiredMuted: state.desiredMuted,
    desiredPaused: state.desiredPaused,
    desiredSeekPositionSeconds: state.desiredSeekPositionSeconds,
    desiredVolume: state.desiredVolume,
    durationSeconds: state.durationSeconds,
    failureMessage: state.failure?.message,
    phase:
      phase === 'preparing' ||
      phase === 'loading' ||
      phase === 'playing' ||
      phase === 'paused' ||
      phase === 'ended' ||
      phase === 'failed'
        ? phase
        : 'preparing',
    positionSeconds: state.positionSeconds ?? 0,
    generation: state.generation,
    sessionId: state.sessionId,
    media: state.media,
    subtitle: state.subtitle,
    timelineOffsetSeconds: state.timelineOffsetSeconds ?? 0,
    title: state.title,
  };
}

function browserCapabilities(): WebPlaybackCapabilities {
  const video = document.createElement('video');
  const supports = (mimeType: string) =>
    (typeof MediaSource !== 'undefined' && MediaSource.isTypeSupported(mimeType)) ||
    video.canPlayType(mimeType).length > 0;
  const aac = supports('audio/mp4; codecs="mp4a.40.2"');

  return {
    aac,
    fragmentedMp4Hls:
      HlsModule.default.isSupported() ||
      video.canPlayType('application/vnd.apple.mpegurl').length > 0,
    h264Sdr: supports('video/mp4; codecs="avc1.640028"'),
    hevcMain10Hdr: supports('video/mp4; codecs="hvc1.2.4.L153.B0"'),
    maxAudioChannels: aac ? 8 : 0,
  };
}

function controlCommandFor(command: EmbeddedPlayerControl): PlaybackControlCommand {
  if (command.kind === 'seek') {
    return { kind: 'seek', position_seconds: command.positionSeconds };
  }
  return command;
}

function observationKindFor(observation: EmbeddedPlayerObservation): EmbeddedPlayerObservationKind {
  if (observation.kind === 'failed') {
    return {
      kind: 'failed',
      message: observation.message ?? 'Browser playback failed',
    };
  }
  return { kind: observation.kind };
}

export function EmbeddedPlayerProvider(props: {
  children: JSX.Element;
  onActivePlayerChanged?: () => void;
}) {
  const [state, setState] = createSignal<EmbeddedPlayerState | null>(null);
  const [settled, setSettled] = createSignal(false);
  let observationSequence = 0;
  let receivedStateEvent = false;
  let activeNavigationKey: string | null = null;

  const player = createMemo(() => viewModelFromState(state()));
  const sessionKey = createMemo(() => {
    const current = state();
    if (!current || current.sessionId === null || current.generation === null) {
      return null;
    }
    return `${current.sessionId}:${current.generation.toString()}`;
  });
  const commit = (nextState: EmbeddedPlayerState) => {
    const current = state();
    if (current && nextState.revision < current.revision) {
      return;
    }
    setState(nextState);
    if (hasActiveSession(nextState)) {
      const nextKey = `${nextState.sessionId}:${nextState.generation.toString()}`;
      if (nextKey !== activeNavigationKey) {
        activeNavigationKey = nextKey;
        props.onActivePlayerChanged?.();
      }
    } else {
      activeNavigationKey = null;
    }
  };

  const runState = (effect: EmbeddedPlayerEffect<EmbeddedPlayerState>) => {
    void Effect.runPromiseExit(effect).then((exit) => {
      if (Exit.isSuccess(exit)) {
        commit(exit.value);
      }
    });
  };

  createEffect(() => {
    sessionKey();
    observationSequence = 0;
  });

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;

    void Effect.runPromiseExit(fetchEmbeddedPlayerState).then((exit) => {
      if (Exit.isSuccess(exit) && !receivedStateEvent) {
        commit(exit.value);
      }
      setSettled(true);
    });
    void Effect.runPromiseExit(registerEmbeddedPlayerCapabilities(browserCapabilities())).then(
      (exit) => {
        if (Exit.isSuccess(exit) && !receivedStateEvent) {
          commit(exit.value);
        }
      },
    );
    void listenEmbeddedPlayerChanged((nextState) => {
      receivedStateEvent = true;
      commit(nextState);
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

  const control = (command: EmbeddedPlayerControl) =>
    runState(controlEmbeddedPlayer(controlCommandFor(command)));
  const observe = (observation: EmbeddedPlayerObservation) => {
    const current = state();
    if (!current || current.sessionId === null || current.generation === null) {
      return;
    }
    observationSequence += 1;
    const payload: TauriEmbeddedPlayerObservation = {
      durationSeconds: observation.durationSeconds,
      generation: observation.generation,
      kind: observationKindFor(observation),
      mediaTimeSeconds: observation.mediaTimeSeconds,
      muted: observation.muted,
      seekableEndSeconds: observation.seekableEndSeconds,
      seekableStartSeconds: observation.seekableStartSeconds,
      sequence: observationSequence,
      sessionId: observation.sessionId,
      volume: Math.round(Math.min(100, Math.max(0, observation.volume))),
    };
    runState(observeEmbeddedPlayer(payload));
  };
  const playInMpv = () => {
    void Effect.runPromiseExit(playEmbeddedPlayerInMpv);
  };

  const value: EmbeddedPlayerOwner = { control, observe, playInMpv, player, settled };
  return (
    <EmbeddedPlayerContext.Provider value={value}>{props.children}</EmbeddedPlayerContext.Provider>
  );
}
