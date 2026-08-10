import type { EmbeddedPlayerMedia } from '@bindings';
import { Button } from '@components/ui';
import { cx } from '@styled-system/css';
import * as HlsModule from 'hls.js';
import {
  AlertCircle,
  LoaderCircle,
  Maximize,
  Minimize,
  MonitorPlay,
  Pause,
  Play,
  RotateCcw,
  Volume2,
  VolumeX,
  X,
} from 'lucide-solid';
import { Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import type { Accessor, JSX } from 'solid-js';

import * as styles from './EmbeddedPlayer.styles';

export interface EmbeddedPlayerViewModel {
  /** Native session identity used to reject observations from replaced media. */
  sessionId: string;
  /** Native pipeline generation used to reject observations after seek/restart. */
  generation: number;
  /** Browser-playable media selected for the current Jellyfin item, or null while unavailable. */
  media: EmbeddedPlayerMedia | null;
  /** Real Jellyfin media title displayed in the player chrome. */
  title: string;
  /** Optional real context, such as the series and episode number. */
  subtitle?: string | null;
  /** Optional artwork supplied by Jellyfin for the native video poster. */
  posterUrl?: string | null;
  phase: 'preparing' | 'loading' | 'playing' | 'paused' | 'ended' | 'failed';
  timelineOffsetSeconds: number;
  positionSeconds: number;
  durationSeconds: number | null;
  desiredPaused: boolean;
  desiredMuted: boolean;
  desiredVolume: number;
  desiredSeekPositionSeconds: number | null;
  failureMessage?: string | null;
  canPlayInMpv: boolean;
}

export type EmbeddedPlayerControl =
  | { kind: 'pause' }
  | { kind: 'resume' }
  | { kind: 'seek'; positionSeconds: number }
  | { kind: 'setVolume'; volume: number }
  | { kind: 'toggleMute' }
  | { kind: 'stop' }
  | { kind: 'restart' }
  | { kind: 'replay' };

export interface EmbeddedPlayerObservation {
  sessionId: string;
  generation: number;
  kind: 'ready' | 'playing' | 'paused' | 'buffering' | 'ended' | 'failed';
  mediaTimeSeconds: number;
  durationSeconds: number | null;
  seekableStartSeconds: number | null;
  seekableEndSeconds: number | null;
  muted: boolean;
  volume: number;
  message?: string;
}

export interface EmbeddedPlayerEffects {
  /** Starts the existing external-MPV playback path for this item. */
  onPlayInMpv?: () => void;
  onControl?: (command: EmbeddedPlayerControl) => void;
  onObservation?: (observation: EmbeddedPlayerObservation) => void;
  onExit?: () => void;
}

export interface EmbeddedPlayerProps {
  /** A reactive player model owned by the route/effect integration layer. */
  player: Accessor<EmbeddedPlayerViewModel | null>;
  effects?: EmbeddedPlayerEffects;
}

interface MediaSession {
  durationSeconds: number | null;
  generation: number;
  key: string;
  media: EmbeddedPlayerMedia;
  sessionId: string;
}

function formatTime(seconds: number): string {
  const totalSeconds = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const remainingSeconds = totalSeconds % 60;
  const minuteText = hours > 0 ? minutes.toString().padStart(2, '0') : minutes.toString();
  return `${hours > 0 ? `${hours.toString()}:` : ''}${minuteText}:${remainingSeconds
    .toString()
    .padStart(2, '0')}`;
}

export default function EmbeddedPlayer(props: EmbeddedPlayerProps) {
  let videoElement: HTMLVideoElement | undefined;
  const [status, setStatus] = createSignal<'idle' | 'loading' | 'ready' | 'error' | 'ended'>(
    'idle',
  );
  const [currentTime, setCurrentTime] = createSignal(0);
  const [duration, setDuration] = createSignal(0);
  const [volume, setVolume] = createSignal(100);
  const [muted, setMuted] = createSignal(false);
  const [paused, setPaused] = createSignal(true);
  const [fullscreen, setFullscreen] = createSignal(false);
  const [errorMessage, setErrorMessage] = createSignal<string | null>(null);
  let stallTimer: ReturnType<typeof setTimeout> | undefined;

  const model = () => props.player();
  const mediaSource = createMemo(() => model()?.media ?? null);
  const mediaSession = createMemo<MediaSession | null>((previous) => {
    const current = model();
    if (!current?.media) {
      return null;
    }
    const key = `${current.sessionId}:${current.generation.toString()}:${current.media.kind}:${current.media.url}`;
    if (previous?.key === key) {
      return previous;
    }
    return {
      durationSeconds: current.durationSeconds,
      generation: current.generation,
      key,
      media: current.media,
      sessionId: current.sessionId,
    };
  }, null);
  const hasSource = () => mediaSource() !== null;
  const timelineAvailable = createMemo(() => Number.isFinite(duration()) && duration() > 0);
  const seekValue = createMemo(() => (timelineAvailable() ? currentTime() : 0));
  const isPlaying = () => status() === 'ready' && !paused();

  const seekableRange = (video: HTMLVideoElement) => {
    if (video.seekable.length === 0) {
      return { end: null, start: null };
    }
    return {
      end: video.seekable.end(video.seekable.length - 1),
      start: video.seekable.start(0),
    };
  };

  const isCurrentMediaSession = (session: MediaSession, video: HTMLVideoElement) =>
    videoElement === video && mediaSession()?.key === session.key;

  const observe = (
    session: MediaSession,
    video: HTMLVideoElement,
    kind: EmbeddedPlayerObservation['kind'],
    message?: string,
  ) => {
    const range = seekableRange(video);
    props.effects?.onObservation?.({
      durationSeconds:
        session.durationSeconds !== null && Number.isFinite(session.durationSeconds)
          ? session.durationSeconds
          : Number.isFinite(video.duration)
            ? video.duration
            : null,
      kind,
      generation: session.generation,
      mediaTimeSeconds: video.currentTime,
      message,
      muted: video.muted,
      sessionId: session.sessionId,
      seekableEndSeconds: range.end,
      seekableStartSeconds: range.start,
      volume: Math.round(video.volume * 100),
    });
  };

  const clearStallTimer = () => {
    if (stallTimer !== undefined) {
      clearTimeout(stallTimer);
      stallTimer = undefined;
    }
  };

  const startStallTimer = (session: MediaSession, video: HTMLVideoElement) => {
    if (!isCurrentMediaSession(session, video)) {
      return;
    }
    clearStallTimer();
    stallTimer = setTimeout(() => {
      handleVideoError(session, video, 'Playback stalled while waiting for transcoded media.');
    }, 15_000);
  };

  createEffect(() => {
    const session = mediaSession();
    setCurrentTime(0);
    setDuration(0);
    setPaused(true);
    setErrorMessage(null);
    setStatus(session ? 'loading' : 'idle');
    if (!videoElement || !session) {
      return;
    }
    const video = videoElement;

    let hls: InstanceType<typeof HlsModule.default> | null = null;
    if (session.media.kind === 'directSource') {
      video.src = session.media.url;
      video.load();
    } else if (HlsModule.default.isSupported()) {
      hls = new HlsModule.default({
        enableWorker: true,
        lowLatencyMode: false,
      });
      hls.on(HlsModule.Events.ERROR, (_event, data) => {
        if (data.fatal) {
          handleVideoError(session, video, data.details);
        }
      });
      hls.loadSource(session.media.url);
      hls.attachMedia(video);
    } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = session.media.url;
      video.load();
    } else {
      handleVideoError(session, video, 'This system WebView does not support HLS playback.');
    }

    onCleanup(() => {
      clearStallTimer();
      hls?.destroy();
      video.removeAttribute('src');
      video.load();
    });
  });

  createEffect(() => {
    const current = model();
    if (!videoElement || !current?.media) {
      return;
    }
    videoElement.muted = current.desiredMuted;
    videoElement.volume = Math.min(1, Math.max(0, current.desiredVolume / 100));
    if (current.desiredPaused) {
      videoElement.pause();
    } else if (videoElement.paused) {
      void videoElement.play().catch(() => undefined);
    }
  });

  createEffect(() => {
    const current = model();
    const requested = current?.desiredSeekPositionSeconds;
    if (!videoElement || requested === null || requested === undefined || !current) {
      return;
    }
    const relative = Math.max(0, requested - current.timelineOffsetSeconds);
    if (Math.abs(videoElement.currentTime - relative) > 0.25) {
      videoElement.currentTime = relative;
    }
  });

  onMount(() => {
    const updateFullscreen = () =>
      setFullscreen(document.fullscreenElement === videoElement?.parentElement);
    document.addEventListener('fullscreenchange', updateFullscreen);
    onCleanup(() => {
      clearStallTimer();
      document.removeEventListener('fullscreenchange', updateFullscreen);
    });
  });

  const handleLoadedMetadata = (session: MediaSession, video: HTMLVideoElement) => {
    if (isCurrentMediaSession(session, video)) {
      clearStallTimer();
      const nextDuration = video.duration;
      const sourceDuration = model()?.durationSeconds;
      setDuration(
        sourceDuration !== null && sourceDuration !== undefined && Number.isFinite(sourceDuration)
          ? sourceDuration
          : Number.isFinite(nextDuration)
            ? nextDuration
            : 0,
      );
      setStatus('ready');
      const current = model();
      if (current && current.positionSeconds > current.timelineOffsetSeconds) {
        video.currentTime = current.positionSeconds - current.timelineOffsetSeconds;
      }
    }
    observe(session, video, 'ready');
  };

  const handleTimeUpdate = (session: MediaSession, video: HTMLVideoElement) => {
    if (isCurrentMediaSession(session, video)) {
      setCurrentTime(video.currentTime + (model()?.timelineOffsetSeconds ?? 0));
    }
    observe(session, video, video.paused ? 'paused' : 'playing');
  };

  const handleVolumeChange = (session: MediaSession, video: HTMLVideoElement) => {
    if (isCurrentMediaSession(session, video)) {
      setVolume(Math.round(video.volume * 100));
      setMuted(video.muted);
    }
  };

  const handleVideoError = (
    session: MediaSession,
    video: HTMLVideoElement,
    detail = 'This stream could not be played in the embedded player.',
  ) => {
    const message = detail;
    if (isCurrentMediaSession(session, video)) {
      setErrorMessage(message);
      setStatus('error');
    }
    observe(session, video, 'failed', message);
  };

  const togglePlayback = async () => {
    if (!videoElement || !hasSource()) {
      return;
    }
    if (videoElement.paused) {
      props.effects?.onControl?.({ kind: 'resume' });
      try {
        await videoElement.play();
      } catch {
        const session = mediaSession();
        if (session) {
          handleVideoError(session, videoElement);
        }
      }
      return;
    }
    props.effects?.onControl?.({ kind: 'pause' });
    videoElement.pause();
  };

  const seek = (value: number) => {
    if (!videoElement || !timelineAvailable()) {
      return;
    }
    videoElement.currentTime = Math.max(0, value - (model()?.timelineOffsetSeconds ?? 0));
    setCurrentTime(value);
    props.effects?.onControl?.({
      kind: 'seek',
      positionSeconds: value,
    });
  };

  const changeVolume = (value: number) => {
    if (!videoElement) {
      return;
    }
    videoElement.volume = value / 100;
    if (value > 0 && videoElement.muted) {
      videoElement.muted = false;
    }
    props.effects?.onControl?.({ kind: 'setVolume', volume: value });
  };

  const toggleMute = () => {
    if (videoElement) {
      props.effects?.onControl?.({ kind: 'toggleMute' });
      videoElement.muted = !videoElement.muted;
    }
  };

  const toggleFullscreen = async () => {
    const playerRoot = videoElement?.parentElement;
    if (!playerRoot) {
      return;
    }
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
      } else {
        await playerRoot.requestFullscreen();
      }
    } catch {
      // Fullscreen availability is host-controlled (including Tauri WebViews).
    }
  };

  const retry = () => {
    if (!model()) {
      return;
    }
    setErrorMessage(null);
    setStatus('loading');
    props.effects?.onControl?.({ kind: 'restart' });
  };

  const replay = async () => {
    if (!videoElement) {
      return;
    }
    videoElement.currentTime = 0;
    setCurrentTime(0);
    props.effects?.onControl?.({ kind: 'replay' });
    try {
      await videoElement.play();
    } catch {
      const session = mediaSession();
      if (session) {
        handleVideoError(session, videoElement);
      }
    }
  };

  return (
    <section class={styles.root} aria-label="Embedded web player">
      {/* The first embedded-player slice intentionally excludes subtitle and caption tracks. */}
      <Show when={mediaSession()} keyed>
        {(session) => (
          // oxlint-disable-next-line jsx-a11y/media-has-caption
          <video
            ref={(element) => {
              videoElement = element;
            }}
            class={styles.video}
            poster={model()?.posterUrl ?? undefined}
            preload="metadata"
            playsinline
            onCanPlay={(event) => {
              if (isCurrentMediaSession(session, event.currentTarget)) {
                clearStallTimer();
                setStatus('ready');
              }
            }}
            onEnded={(event) => {
              const video = event.currentTarget;
              if (isCurrentMediaSession(session, video)) {
                setStatus('ended');
              }
              observe(session, video, 'ended');
            }}
            onError={(event) => handleVideoError(session, event.currentTarget)}
            onLoadedMetadata={(event) => handleLoadedMetadata(session, event.currentTarget)}
            onPause={(event) => {
              const video = event.currentTarget;
              if (isCurrentMediaSession(session, video)) {
                setPaused(true);
              }
              observe(session, video, 'paused');
            }}
            onPlay={(event) => {
              const video = event.currentTarget;
              if (isCurrentMediaSession(session, video)) {
                clearStallTimer();
                setPaused(false);
                setStatus('ready');
              }
              observe(session, video, 'playing');
            }}
            onTimeUpdate={(event) => handleTimeUpdate(session, event.currentTarget)}
            onVolumeChange={(event) => handleVolumeChange(session, event.currentTarget)}
            onWaiting={(event) => {
              const video = event.currentTarget;
              if (isCurrentMediaSession(session, video)) {
                setStatus('loading');
              }
              observe(session, video, 'buffering');
              startStallTimer(session, video);
            }}
          />
        )}
      </Show>

      <div aria-hidden="true" class={styles.scrim} />

      <Show when={model()}>
        <button
          class={cx(styles.controlButton(), styles.exitButton)}
          type="button"
          aria-label="Stop playback and close player"
          onClick={() => {
            props.effects?.onControl?.({ kind: 'stop' });
            props.effects?.onExit?.();
          }}
        >
          <X class={styles.controlIcon} />
        </button>
      </Show>

      <Show when={model()}>
        {(current) => (
          <header class={styles.titleBar}>
            <span class={styles.eyebrow}>Embedded web player</span>
            <h1 class={styles.title}>{current().title}</h1>
            <Show when={current().subtitle}>
              <p class={styles.subtitle}>{current().subtitle}</p>
            </Show>
          </header>
        )}
      </Show>

      <Show when={status() === 'idle' && !model()}>
        <PlayerState
          icon={<MonitorPlay class={styles.stateIcon} />}
          title="Web player unavailable"
          message="Choose a supported movie or episode to start embedded playback."
        />
      </Show>
      <Show
        when={
          (status() === 'loading' ||
            model()?.phase === 'preparing' ||
            model()?.phase === 'loading') &&
          model()?.phase !== 'failed'
        }
      >
        <PlayerState
          icon={<LoaderCircle class={styles.loadingIndicator} />}
          title="Loading stream"
          message="Preparing this item for browser playback."
          onPlayInMpv={model()?.canPlayInMpv ? props.effects?.onPlayInMpv : undefined}
        />
      </Show>
      <Show when={status() === 'error' || model()?.phase === 'failed'}>
        <PlayerState
          icon={<AlertCircle class={styles.errorIcon} />}
          title="Embedded playback failed"
          message={
            model()?.failureMessage ??
            errorMessage() ??
            'This stream could not be played in the embedded player.'
          }
          onPlayInMpv={model()?.canPlayInMpv ? props.effects?.onPlayInMpv : undefined}
          onRetry={retry}
        />
      </Show>
      <Show when={status() === 'ended' || model()?.phase === 'ended'}>
        <PlayerState
          icon={<RotateCcw class={styles.stateIcon} />}
          title="Playback finished"
          message="Replay this item or continue playback in MPV."
          onPlayInMpv={model()?.canPlayInMpv ? props.effects?.onPlayInMpv : undefined}
          onReplay={() => void replay()}
        />
      </Show>

      <div class={styles.controls}>
        <div class={styles.timelineRow}>
          <span class={styles.time}>{formatTime(currentTime())}</span>
          <input
            class={styles.range}
            aria-label="Seek position"
            type="range"
            min="0"
            max={duration()}
            step="0.1"
            value={seekValue()}
            disabled={!timelineAvailable() || !hasSource()}
            onInput={(event) => seek(Number(event.currentTarget.value))}
          />
          <span class={styles.time}>{timelineAvailable() ? formatTime(duration()) : '—'}</span>
        </div>

        <div class={styles.controlRow}>
          <div class={styles.controlCluster}>
            <button
              class={styles.controlButton({ primary: true })}
              type="button"
              aria-label={isPlaying() ? 'Pause' : 'Play'}
              disabled={!hasSource()}
              onClick={() => void togglePlayback()}
            >
              <Show when={isPlaying()} fallback={<Play class={styles.controlIcon} />}>
                <Pause class={styles.controlIcon} />
              </Show>
            </button>
            <button
              class={styles.controlButton()}
              type="button"
              aria-label={muted() ? 'Unmute' : 'Mute'}
              disabled={!hasSource()}
              onClick={toggleMute}
            >
              <Show when={muted()} fallback={<Volume2 class={styles.controlIcon} />}>
                <VolumeX class={styles.controlIcon} />
              </Show>
            </button>
            <input
              class={styles.volume}
              aria-label="Volume"
              type="range"
              min="0"
              max="100"
              step="1"
              value={volume()}
              disabled={!hasSource()}
              onInput={(event) => changeVolume(Number(event.currentTarget.value))}
            />
            <span class={styles.compactVolume}>{volume()}%</span>
          </div>

          <div class={styles.controlCluster}>
            <button
              class={styles.controlButton()}
              type="button"
              aria-label={fullscreen() ? 'Exit fullscreen' : 'Enter fullscreen'}
              disabled={!hasSource()}
              onClick={() => void toggleFullscreen()}
            >
              <Show when={fullscreen()} fallback={<Maximize class={styles.controlIcon} />}>
                <Minimize class={styles.controlIcon} />
              </Show>
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}

function PlayerState(props: {
  icon: JSX.Element;
  title: string;
  message: string;
  onPlayInMpv?: () => void;
  onReplay?: () => void;
  onRetry?: () => void;
}) {
  return (
    <div
      class={styles.stateOverlay}
      role={props.title === 'Embedded playback failed' ? 'alert' : 'status'}
    >
      <div class={styles.statePanel}>
        {props.icon}
        <h2 class={styles.stateTitle}>{props.title}</h2>
        <p class={styles.stateMessage}>{props.message}</p>
        <div class={styles.stateActions}>
          <Show when={props.onReplay}>
            <Button type="button" variant="primary" onClick={props.onReplay} leadingIcon={<Play />}>
              Replay
            </Button>
          </Show>
          <Show when={props.onRetry}>
            <Button
              type="button"
              variant="outlined"
              onClick={props.onRetry}
              leadingIcon={<RotateCcw />}
            >
              Try again
            </Button>
          </Show>
          <Button
            type="button"
            variant={props.onReplay || props.onRetry ? 'outlined' : 'primary'}
            disabled={!props.onPlayInMpv}
            onClick={props.onPlayInMpv}
            leadingIcon={<MonitorPlay />}
          >
            Play in MPV
          </Button>
        </div>
      </div>
    </div>
  );
}
