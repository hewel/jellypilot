import {
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Square,
  Volume2,
  VolumeX,
} from 'lucide-solid';
import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { commands, events, type NowPlayingState } from '../bindings';
import { useToast } from './ToastProvider';
import { StatusBadge } from './ui';

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return '0:00';
  const total = Math.floor(seconds);
  const minutes = Math.floor(total / 60);
  const remaining = total % 60;
  return `${minutes}:${remaining.toString().padStart(2, '0')}`;
}

function unavailableCopy(reason?: string | null): string {
  switch (reason) {
    case 'noSession':
    case 'noCurrentItem':
    case 'notEpisode':
      return 'Available during episode playback';
    default:
      return 'Unavailable right now';
  }
}

function statusLabel(status: NowPlayingState['status']): string {
  switch (status) {
    case 'offline':
      return 'Player bridge offline';
    case 'idle':
      return 'MPV idle';
    case 'playing':
      return 'Playing';
    case 'paused':
      return 'Paused';
    default:
      return 'Playback state unknown';
  }
}

function statusVariant(status: NowPlayingState['status']) {
  switch (status) {
    case 'playing':
    case 'paused':
      return 'success' as const;
    case 'offline':
    case 'unknown':
      return 'warning' as const;
    default:
      return 'neutral' as const;
  }
}

export default function NowPlayingCard(props: {
  jellyfinConnected: boolean;
  onPlayerStarted?: () => void;
}) {
  const { showToast } = useToast();
  const [state, setState] = createSignal<NowPlayingState | null>(null);
  const [busy, setBusy] = createSignal<string | null>(null);

  const loadState = async () => {
    const result = await commands.nowPlayingGetState();
    if (result.status === 'ok') {
      setState(result.data);
    }
  };

  const runCommand = async (
    key: string,
    command: () => Promise<
      { status: 'ok' } | { status: 'error'; error: { message: string } }
    >,
    failure: string,
  ) => {
    setBusy(key);
    try {
      const result = await command();
      if (result.status === 'error') {
        showToast('error', result.error.message || failure);
      }
      await loadState();
    } catch (error) {
      showToast('error', error instanceof Error ? error.message : failure);
    } finally {
      setBusy(null);
    }
  };

  onMount(() => {
    void loadState();
    let disposed = false;
    let cleanup: (() => void) | undefined;

    events.nowPlayingChanged
      .listen((event) => {
        setState(event.payload.state);
      })
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

  const current = () => state();
  const player = () => current()?.player;
  const connected = () => player()?.connected ?? false;
  const activeTimeline = () => {
    const duration = player()?.duration ?? 0;
    return connected() && Number.isFinite(duration) && duration > 0;
  };
  const mediaTitle = () =>
    current()?.media?.name ?? 'No active playback metadata';
  const mediaSubtitle = () => {
    const media = current()?.media;
    if (!media)
      return props.jellyfinConnected
        ? 'Waiting for Jellyfin playback'
        : 'Reconnect Jellyfin before starting MPV';
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
    <section
      class="card-elevated space-y-5"
      aria-labelledby="now-playing-title"
    >
      <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p class="text-label-medium uppercase text-secondary">Now Playing</p>
          <h2
            id="now-playing-title"
            class="text-headline-small text-on-surface"
          >
            {mediaTitle()}
          </h2>
          <p class="text-body-medium text-on-surface-variant">
            {mediaSubtitle()}
          </p>
        </div>
        <StatusBadge variant={statusVariant(current()?.status ?? 'unknown')}>
          {statusLabel(current()?.status ?? 'unknown')}
        </StatusBadge>
      </div>

      <div class="rounded-3xl border border-outline-variant/60 bg-surface-container-lowest p-4">
        <div class="mb-3 flex items-center justify-between text-label-small text-on-surface-variant">
          <span>{formatTime(player()?.timePos ?? 0)}</span>
          <span>
            {activeTimeline()
              ? formatTime(player()?.duration ?? 0)
              : 'Timeline unavailable'}
          </span>
        </div>
        <Show
          when={activeTimeline()}
          fallback={<div class="h-2 rounded-full bg-surface-container-high" />}
        >
          <input
            aria-label="Seek position"
            type="range"
            min="0"
            max={player()?.duration ?? 0}
            value={player()?.timePos ?? 0}
            disabled={!connected() || busy() !== null}
            class="w-full accent-primary"
            onChange={(event) => {
              const time = Number(event.currentTarget.value);
              void runCommand(
                'seek',
                () => commands.mpvSeek(time),
                'Could not seek playback',
              );
            }}
          />
        </Show>
      </div>

      <div class="flex flex-wrap items-center gap-3">
        <button
          type="button"
          class="btn-icon"
          aria-label="Previous episode"
          title={
            current()?.canPlayPrevious
              ? 'Previous episode'
              : unavailableCopy(current()?.previousUnavailableReason)
          }
          disabled={!current()?.canPlayPrevious || busy() !== null}
          onClick={() =>
            void runCommand(
              'previous',
              commands.jellyfinPlayPreviousEpisode,
              'Could not play previous episode',
            )
          }
        >
          <SkipBack class="h-5 w-5" />
        </button>
        <button
          type="button"
          class="btn-primary min-w-32"
          disabled={!connected() || busy() !== null}
          onClick={() =>
            void runCommand(
              'pause',
              () => commands.mpvSetPause(!(player()?.paused ?? true)),
              'Could not change playback state',
            )
          }
        >
          <Show
            when={player()?.paused ?? true}
            fallback={<Pause class="h-5 w-5" />}
          >
            <Play class="h-5 w-5" />
          </Show>
          {player()?.paused ? 'Play' : 'Pause'}
        </button>
        <button
          type="button"
          class="btn-icon"
          aria-label="Stop playback"
          disabled={!connected() || busy() !== null}
          onClick={() =>
            void runCommand('stop', commands.mpvStop, 'Could not stop MPV')
          }
        >
          <Square class="h-5 w-5" />
        </button>
        <button
          type="button"
          class="btn-icon"
          aria-label="Next episode"
          title={
            current()?.canPlayNext
              ? 'Next episode'
              : unavailableCopy(current()?.nextUnavailableReason)
          }
          disabled={!current()?.canPlayNext || busy() !== null}
          onClick={() =>
            void runCommand(
              'next',
              commands.jellyfinPlayNextEpisode,
              'Could not play next episode',
            )
          }
        >
          <SkipForward class="h-5 w-5" />
        </button>
        <Show when={!connected()}>
          <button
            type="button"
            class="btn-secondary"
            disabled={!props.jellyfinConnected || busy() !== null}
            onClick={() =>
              void runCommand(
                'start',
                commands.mpvStart,
                'Could not start MPV',
              ).then(() => props.onPlayerStarted?.())
            }
          >
            <Play class="h-5 w-5" />
            {props.jellyfinConnected ? 'Start MPV' : 'Reconnect Jellyfin first'}
          </button>
        </Show>
      </div>

      <div class="flex flex-col gap-3 rounded-3xl border border-outline-variant/60 bg-surface-container-lowest p-4 sm:flex-row sm:items-center">
        <button
          type="button"
          class="btn-icon shrink-0"
          aria-label="Mute"
          disabled={!connected() || busy() !== null}
          onClick={() =>
            void runCommand(
              'mute',
              commands.mpvToggleMute,
              'Could not toggle mute',
            )
          }
        >
          <Show when={connected()} fallback={<VolumeX class="h-5 w-5" />}>
            <Volume2 class="h-5 w-5" />
          </Show>
        </button>
        <input
          aria-label="Volume"
          type="range"
          min="0"
          max="100"
          value={player()?.volume ?? 100}
          disabled={!connected() || busy() !== null}
          class="w-full accent-secondary"
          onChange={(event) => {
            const volume = Number(event.currentTarget.value);
            void runCommand(
              'volume',
              () => commands.mpvSetVolume(volume),
              'Could not set volume',
            );
          }}
        />
        <span class="w-12 text-right font-mono text-body-small text-on-surface-variant">
          {Math.round(player()?.volume ?? 100)}%
        </span>
      </div>
    </section>
  );
}
