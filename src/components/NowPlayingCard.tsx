import { Slider } from '@ark-ui/solid/slider';
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
  const [seekDraft, setSeekDraft] = createSignal<number | null>(null);
  const [volumeDraft, setVolumeDraft] = createSignal<number | null>(null);

  const loadState = async () => {
    const result = await commands.nowPlayingGetState();
    if (result.status === 'ok') {
      setState(result.data);
      setSeekDraft(null);
      setVolumeDraft(null);
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
        setSeekDraft(null);
        setVolumeDraft(null);
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
  const seekValue = () => seekDraft() ?? player()?.timePos ?? 0;
  const volumeValue = () => volumeDraft() ?? player()?.volume ?? 100;
  const muted = () => player()?.muted ?? false;
  const activeTimeline = () => {
    const duration = player()?.duration ?? 0;
    return connected() && Number.isFinite(duration) && duration > 0;
  };
  const canControlPlayback = () => {
    const status = current()?.status;
    return status === 'playing' || status === 'paused';
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
  const commitSeek = (value: number) => {
    if (!activeTimeline() || !canControlPlayback() || busy() !== null) return;
    void runCommand(
      'seek',
      () => commands.mpvSeek(value),
      'Could not seek playback',
    );
  };

  const commitVolume = (value: number) => {
    if (!connected() || busy() !== null) return;
    void runCommand(
      'volume',
      () => commands.mpvSetVolume(value),
      'Could not set volume',
    );
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
          <span>{formatTime(seekValue())}</span>
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
          <Slider.Root
            aria-label={['Seek position']}
            min={0}
            max={player()?.duration ?? 0}
            value={[seekValue()]}
            disabled={
              !activeTimeline() || !canControlPlayback() || busy() !== null
            }
            onValueChange={(details) => setSeekDraft(details.value[0] ?? 0)}
            onValueChangeEnd={(details) => commitSeek(details.value[0] ?? 0)}
            class="ark-slider"
          >
            <Slider.Control class="ark-slider__control">
              <Slider.Track class="ark-slider__track">
                <Slider.Range class="ark-slider__range bg-primary" />
              </Slider.Track>
              <Slider.Thumb index={0} class="ark-slider__thumb">
                <Slider.HiddenInput />
              </Slider.Thumb>
            </Slider.Control>
          </Slider.Root>
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
          disabled={!canControlPlayback() || busy() !== null}
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
          disabled={!canControlPlayback() || busy() !== null}
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
        <Show when={current()?.status === 'offline' && !connected()}>
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
          aria-label={muted() ? 'Unmute' : 'Mute'}
          disabled={!connected() || busy() !== null}
          onClick={() =>
            void runCommand(
              'mute',
              commands.mpvToggleMute,
              'Could not toggle mute',
            )
          }
        >
          <Show when={connected() && !muted()} fallback={<VolumeX class="h-5 w-5" />}>
            <Volume2 class="h-5 w-5" />
          </Show>
        </button>
        <Slider.Root
          aria-label={['Volume']}
          min={0}
          max={100}
          value={[volumeValue()]}
          disabled={!connected() || busy() !== null}
          onValueChange={(details) => setVolumeDraft(details.value[0] ?? 100)}
          onValueChangeEnd={(details) => commitVolume(details.value[0] ?? 100)}
          class="ark-slider flex-1"
        >
          <Slider.Control class="ark-slider__control">
            <Slider.Track class="ark-slider__track">
              <Slider.Range class="ark-slider__range bg-secondary" />
            </Slider.Track>
            <Slider.Thumb index={0} class="ark-slider__thumb">
              <Slider.HiddenInput />
            </Slider.Thumb>
          </Slider.Control>
        </Slider.Root>
        <span class="w-12 text-right font-mono text-body-small text-on-surface-variant">
          {Math.round(volumeValue())}%
        </span>
      </div>
    </section>
  );
}
