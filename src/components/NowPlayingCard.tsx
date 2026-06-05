import { Slider } from '@ark-ui/solid/slider';
import { Effect, Exit } from 'effect';
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
import {
  type CommandError,
  commands,
  events,
  type NowPlayingState,
} from '../bindings';
import { commandFailureMessage, runTauriCommand } from '../effects/commands';
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
      { status: 'ok'; data: null } | { status: 'error'; error: CommandError }
    >,
    failure: string,
  ) => {
    setBusy(key);
    const exit = await Effect.runPromiseExit(runTauriCommand(command));
    if (Exit.isSuccess(exit)) {
      await loadState();
    } else {
      showToast('error', commandFailureMessage(exit.cause, failure));
    }
    setBusy(null);
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
        ? 'Awaiting playback command from Jellyfin'
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
      class="card-elevated space-y-6 relative overflow-hidden group/card"
      aria-labelledby="now-playing-title"
    >
      {/* Decorative subtle ambient card glow */}
      <div class="absolute inset-0 bg-gradient-to-r from-primary/5 to-secondary/5 opacity-0 group-hover/card:opacity-100 transition-opacity duration-500 pointer-events-none" />

      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between relative z-10">
        <div class="space-y-1 max-w-[70%]">
          <p class="text-label-small text-secondary font-bold">Now Playing</p>
          <div class="flex items-center gap-3">
            <h2
              id="now-playing-title"
              class="text-headline-small text-on-surface truncate pr-2"
            >
              {mediaTitle()}
            </h2>
            <Show when={current()?.status === 'playing'}>
              <div
                class="flex items-end gap-1.5 h-6 w-8 pb-1 shrink-0 select-none"
                aria-hidden="true"
                title="Playing stream"
              >
                <span class="w-1.5 h-full bg-primary rounded-full wave-bar animate-wave-1" />
                <span class="w-1.5 h-full bg-secondary rounded-full wave-bar animate-wave-2" />
                <span class="w-1.5 h-full bg-primary rounded-full wave-bar animate-wave-3" />
                <span class="w-1.5 h-full bg-secondary rounded-full wave-bar animate-wave-4" />
              </div>
            </Show>
          </div>
          <p class="text-body-medium text-on-surface-variant font-medium">
            {mediaSubtitle()}
          </p>
        </div>
        <StatusBadge variant={statusVariant(current()?.status ?? 'unknown')}>
          {statusLabel(current()?.status ?? 'unknown')}
        </StatusBadge>
      </div>

      <div class="rounded-2xl border border-outline-variant bg-surface-container-lowest/50 p-4 relative z-10 backdrop-blur-sm shadow-inner">
        <div class="mb-2.5 flex items-center justify-between font-mono text-[11px] font-semibold text-on-surface-variant">
          <span>{formatTime(seekValue())}</span>
          <span>
            {activeTimeline()
              ? formatTime(player()?.duration ?? 0)
              : 'Timeline unavailable'}
          </span>
        </div>
        <Show
          when={activeTimeline()}
          fallback={
            <div class="h-2 rounded-full bg-surface-container-high/60" />
          }
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
                <Slider.Range class="ark-slider__range bg-brand-gradient shadow-brand-glow-sm" />
              </Slider.Track>
              <Slider.Thumb index={0} class="ark-slider__thumb">
                <Slider.HiddenInput />
              </Slider.Thumb>
            </Slider.Control>
          </Slider.Root>
        </Show>
      </div>

      <div class="flex flex-wrap items-center gap-4 relative z-10">
        <button
          type="button"
          class="btn-icon rounded-full border border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary hover:bg-secondary/5"
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
          class="btn-primary min-w-32 rounded-full relative overflow-hidden"
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
            fallback={
              <Pause class="h-5 w-5 drop-shadow-[0_2px_4px_rgba(0,0,0,0.3)]" />
            }
          >
            <Play class="h-5 w-5 drop-shadow-[0_2px_4px_rgba(0,0,0,0.3)]" />
          </Show>
          <span class="font-bold tracking-wide">
            {player()?.paused ? 'Play' : 'Pause'}
          </span>
        </button>
        <button
          type="button"
          class="btn-icon rounded-full border border-outline-variant/60 bg-surface-container-high/30 hover:border-error hover:text-error hover:bg-error/5"
          aria-label="Stop playback"
          disabled={!canControlPlayback() || busy() !== null}
          onClick={() =>
            void runCommand('stop', commands.mpvStop, 'Could not stop MPV')
          }
        >
          <Square class="h-4 w-4 fill-current" />
        </button>
        <button
          type="button"
          class="btn-icon rounded-full border border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary hover:bg-secondary/5"
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
            class="btn-secondary rounded-full"
            disabled={!props.jellyfinConnected || busy() !== null}
            onClick={() =>
              void runCommand(
                'start',
                commands.mpvStart,
                'Could not start MPV',
              ).then(() => props.onPlayerStarted?.())
            }
          >
            <Play class="h-4.5 w-4.5 fill-current" />
            <span>
              {props.jellyfinConnected
                ? 'Start MPV'
                : 'Reconnect Jellyfin first'}
            </span>
          </button>
        </Show>
      </div>

      <div class="flex flex-col gap-3 rounded-2xl border border-outline-variant bg-surface-container-lowest/50 p-4 sm:flex-row sm:items-center relative z-10 backdrop-blur-sm shadow-inner">
        <button
          type="button"
          class="btn-icon shrink-0 rounded-xl hover:bg-secondary/15 hover:text-secondary border border-transparent hover:border-secondary/20"
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
          <Show
            when={connected() && !muted()}
            fallback={<VolumeX class="h-5 w-5 text-error" />}
          >
            <Volume2 class="h-5 w-5 text-secondary" />
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
              <Slider.Range class="ark-slider__range bg-gradient-to-r from-secondary to-[#00abff] shadow-[0_0_8px_rgba(57,213,255,0.4)]" />
            </Slider.Track>
            <Slider.Thumb index={0} class="ark-slider__thumb">
              <Slider.HiddenInput />
            </Slider.Thumb>
          </Slider.Control>
        </Slider.Root>
        <span class="w-12 text-right font-mono text-[13px] font-semibold text-secondary drop-shadow-[0_0_6px_rgba(57,213,255,0.15)]">
          {Math.round(volumeValue())}%
        </span>
      </div>
    </section>
  );
}
