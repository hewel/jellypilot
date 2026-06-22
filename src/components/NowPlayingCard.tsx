import { Slider } from '@ark-ui/solid/slider';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Match } from 'effect';
import { Pause, Play, SkipBack, SkipForward, Square, Volume2, VolumeX } from 'lucide-solid';
import { Show, createSignal, onCleanup, onMount } from 'solid-js';

import type { NowPlayingState } from '../bindings';
import { commandFailureMessage } from '../effects/commands';
import {
  fetchNowPlayingState,
  fetchMpvTrackList,
  setAudioTrack,
  setSubtitleTrack,
  seekPlayback,
  setVolume,
  playPreviousEpisode,
  setPause,
  stopMpv,
  playNextEpisode,
  startMpv,
  toggleMute,
  listenNowPlayingChanged,
} from '../effects/nowPlaying';
import type { NowPlayingEffect } from '../effects/nowPlaying';
import { queryKeys, runExit } from '../effects/query';
import { useToast } from './ToastProvider';
import { Button, Card, JellyPilotSelect, StatusBadge } from './ui';

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return '0:00';
  }
  const total = Math.floor(seconds);
  const minutes = Math.floor(total / 60);
  const remaining = total % 60;
  return `${minutes}:${remaining.toString().padStart(2, '0')}`;
}

const unavailableCopy = Match.type<string | null | undefined>().pipe(
  Match.withReturnType<string>(),
  Match.when(
    Match.is('noSession', 'noCurrentItem', 'notEpisode'),
    () => 'Available during episode playback',
  ),
  Match.orElse(() => 'Unavailable right now'),
);

const statusLabel = Match.type<NowPlayingState['status']>().pipe(
  Match.withReturnType<string>(),
  Match.when('offline', () => 'Offline'),
  Match.when('idle', () => 'MPV idle'),
  Match.when('playing', () => 'Playing'),
  Match.when('paused', () => 'Paused'),
  Match.orElse(() => 'Unknown'),
);

const statusVariant = Match.type<NowPlayingState['status']>().pipe(
  Match.withReturnType<'success' | 'warning' | 'neutral'>(),
  Match.when(Match.is('playing', 'paused'), () => 'success'),
  Match.when(Match.is('offline', 'unknown'), () => 'warning'),
  Match.orElse(() => 'neutral'),
);

const contextualIconClass = (visible: boolean, extra = '') =>
  `${extra} absolute h-5 w-5 transition-[filter,opacity,transform] duration-200 ease-[cubic-bezier(0.2,0,0,1)] ${
    visible ? 'scale-100 opacity-100 blur-0' : 'scale-[0.25] opacity-0 blur-[4px]'
  }`;

export default function NowPlayingCard(props: {
  jellyfinConnected: boolean;
  onPlayerStarted?: () => void;
  bare?: boolean;
  trackSelectPortalMount?: HTMLElement;
}) {
  const { showToast } = useToast();
  const queryClient = useQueryClient();
  const nowPlayingQuery = createQuery(() => ({
    queryKey: queryKeys.nowPlayingState,
    queryFn: () => runExit(fetchNowPlayingState()),
  }));
  const [busy, setBusy] = createSignal<string | null>(null);
  const [seekDraft, setSeekDraft] = createSignal<number | null>(null);
  const [volumeDraft, setVolumeDraft] = createSignal<number | null>(null);
  const current = () =>
    nowPlayingQuery.data && Exit.isSuccess(nowPlayingQuery.data)
      ? nowPlayingQuery.data.value
      : null;
  const player = () => current()?.player;
  const connected = () => player()?.connected ?? false;
  const tracksQuery = createQuery(() => ({
    queryKey: queryKeys.mpvTracks(connected()),
    queryFn: () => runExit(fetchMpvTrackList(connected())),
  }));
  const playerCommandMutation = createMutation(() => ({
    mutationFn: (command: () => NowPlayingEffect<void>) => runExit(command()),
  }));
  const tracks = () =>
    tracksQuery.data && Exit.isSuccess(tracksQuery.data) ? tracksQuery.data.value : [];

  const runCommand = async (
    key: string,
    command: () => NowPlayingEffect<void>,
    failure: string,
  ) => {
    setBusy(key);
    const exit = await playerCommandMutation.mutateAsync(command);
    if (Exit.isSuccess(exit)) {
      await nowPlayingQuery.refetch();
      await tracksQuery.refetch();
    } else {
      showToast('error', commandFailureMessage(exit.cause, failure));
    }
    setBusy(null);
  };

  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;

    listenNowPlayingChanged((state) => {
      queryClient.setQueryData(queryKeys.nowPlayingState, Exit.succeed(state));
      setSeekDraft(null);
      setVolumeDraft(null);
      void queryClient.invalidateQueries({ queryKey: queryKeys.mpvTracks(state.player.connected) });
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
  const mediaTitle = () => current()?.media?.name ?? 'No active playback metadata';
  const mediaSubtitle = () => {
    const media = current()?.media;
    if (!media) {
      return props.jellyfinConnected
        ? 'Awaiting playback command from Jellyfin'
        : 'Reconnect Jellyfin before starting MPV';
    }
    if (media.seriesName) {
      const episode =
        media.seasonNumber && media.episodeNumber
          ? `S${media.seasonNumber.toString().padStart(2, '0')}E${media.episodeNumber.toString().padStart(2, '0')}`
          : media.itemType;
      return `${media.seriesName} · ${episode}`;
    }
    return media.itemType;
  };
  const audioTracks = () => tracks().filter((track) => track.type === 'audio');
  const subtitleTracks = () => tracks().filter((track) => track.type === 'sub');
  const audioTrackItems = () =>
    audioTracks().map((track) => ({ label: track.label, value: track.id.toString() }));
  const subtitleTrackItems = () => [
    { label: 'Off', value: '-1' },
    ...subtitleTracks().map((track) => ({ label: track.label, value: track.id.toString() })),
  ];
  const selectedAudioTrackId = () =>
    audioTracks()
      .find((track) => track.selected)
      ?.id.toString() ?? null;
  const selectedSubtitleTrackId = () =>
    subtitleTracks()
      .find((track) => track.selected)
      ?.id.toString() ?? '-1';
  const switchAudioTrack = (value: string) => {
    const id = Number(value);
    if (value.length === 0 || !Number.isFinite(id) || busy() !== null) {
      return;
    }
    void runCommand('audio-track', () => setAudioTrack(id), 'Could not switch audio track');
  };
  const switchSubtitleTrack = (value: string) => {
    const id = Number(value);
    if (value.length === 0 || !Number.isFinite(id) || busy() !== null) {
      return;
    }
    void runCommand(
      'subtitle-track',
      () => setSubtitleTrack(id),
      'Could not switch subtitle track',
    );
  };
  const commitSeek = (value: number) => {
    if (!activeTimeline() || !canControlPlayback() || busy() !== null) {
      return;
    }
    void runCommand('seek', () => seekPlayback(value), 'Could not seek playback');
  };

  const commitVolume = (value: number) => {
    if (!connected() || busy() !== null) {
      return;
    }
    void runCommand('volume', () => setVolume(value), 'Could not set volume');
  };

  const inner = (
    <div class={props.bare ? 'space-y-5' : 'group/card relative space-y-6 overflow-hidden'}>
      {!props.bare && (
        <div class="from-primary/5 to-secondary/5 pointer-events-none absolute inset-0 bg-gradient-to-r opacity-0 transition-opacity duration-500 group-hover/card:opacity-100" />
      )}

      <div class="relative z-10">
        <div
          class="space-y-1"
          classList={{
            'pr-36': props.bare,
            'max-w-[70%]': !props.bare,
          }}
        >
          {!props.bare && (
            <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
              Now Playing
            </p>
          )}
          <div class="flex items-center gap-3">
            <h2
              id="now-playing-title"
              class={`font-display text-on-surface truncate pr-2 font-bold tracking-tight ${props.bare ? 'text-[20px] leading-[28px]' : 'text-[24px] leading-[32px]'}`}
            >
              {mediaTitle()}
            </h2>
            <Show when={current()?.status === 'playing'}>
              <div
                class="flex h-6 w-8 shrink-0 items-end gap-1.5 pb-1 select-none"
                aria-hidden="true"
                title="Playing stream"
              >
                <span class="bg-primary h-full w-1.5 origin-bottom animate-[wave-bounce_0.8s_ease-in-out_infinite_alternate] rounded-full will-change-transform" />
                <span class="bg-secondary h-full w-1.5 origin-bottom animate-[wave-bounce_0.6s_ease-in-out_infinite_alternate] rounded-full will-change-transform [animation-delay:0.15s]" />
                <span class="bg-primary h-full w-1.5 origin-bottom animate-[wave-bounce_0.9s_ease-in-out_infinite_alternate] rounded-full will-change-transform [animation-delay:0.3s]" />
                <span class="bg-secondary h-full w-1.5 origin-bottom animate-[wave-bounce_0.7s_ease-in-out_infinite_alternate] rounded-full will-change-transform [animation-delay:0.45s]" />
              </div>
            </Show>
          </div>
          <p class="text-on-surface-variant text-[14px] leading-[20px] font-medium">
            {mediaSubtitle()}
          </p>
        </div>
        <div class="absolute top-0 right-0">
          <StatusBadge variant={statusVariant(current()?.status ?? 'unknown')}>
            {statusLabel(current()?.status ?? 'unknown')}
          </StatusBadge>
        </div>
      </div>

      <div class="border-outline-variant bg-surface-container-lowest/50 relative z-10 rounded-3xl border p-4 shadow-inner backdrop-blur-sm">
        <div class="text-on-surface-variant mb-2.5 flex items-center justify-between font-mono text-[11px] font-semibold tabular-nums">
          <span>{formatTime(seekValue())}</span>
          <span>
            {activeTimeline() ? formatTime(player()?.duration ?? 0) : 'Timeline unavailable'}
          </span>
        </div>
        <Show
          when={activeTimeline()}
          fallback={<div class="bg-surface-container-high/60 h-2 rounded-full" />}
        >
          <Slider.Root
            aria-label={['Seek position']}
            min={0}
            max={player()?.duration ?? 0}
            value={[seekValue()]}
            disabled={!activeTimeline() || !canControlPlayback() || busy() !== null}
            onValueChange={(details) => setSeekDraft(details.value[0] ?? 0)}
            onValueChangeEnd={(details) => commitSeek(details.value[0] ?? 0)}
            class="flex w-full flex-col gap-2.5 disabled:opacity-50"
          >
            <Slider.Control class="relative flex h-10 cursor-pointer items-center">
              <Slider.Track class="bg-surface-container-highest/80 border-outline-variant/30 h-2.5 flex-1 overflow-hidden rounded-full border">
                <Slider.Range class="from-primary to-primary-gradient-end h-full rounded-full bg-gradient-to-r shadow-[0_0_10px_rgba(79,70,229,0.35)] transition-[width,transform] duration-150" />
              </Slider.Track>
              <Slider.Thumb
                index={0}
                class="border-surface-container-lowest bg-on-surface data-[focus-visible]:ring-primary/50 flex h-5.5 w-5.5 cursor-grab items-center justify-center rounded-full border-2 shadow-lg shadow-black/50 transition-[box-shadow,transform] duration-200 outline-none hover:scale-110 hover:shadow-[0_0_12px_rgba(255,255,255,0.4)] active:cursor-grabbing data-[focus-visible]:ring-2"
              >
                <Slider.HiddenInput />
              </Slider.Thumb>
            </Slider.Control>
          </Slider.Root>
        </Show>
      </div>

      <div
        class={`relative z-10 flex items-center gap-3 ${props.bare ? 'justify-center' : 'flex-wrap gap-4'}`}
      >
        <Button
          type="button"
          variant="icon"
          class="border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary hover:bg-secondary/5 rounded-full border"
          aria-label="Previous episode"
          title={
            current()?.canPlayPrevious
              ? 'Previous episode'
              : unavailableCopy(current()?.previousUnavailableReason)
          }
          disabled={!current()?.canPlayPrevious || busy() !== null}
          onClick={() =>
            void runCommand('previous', playPreviousEpisode, 'Could not play previous episode')
          }
        >
          <SkipBack class="h-5 w-5" />
        </Button>
        <Button
          type="button"
          variant="primary"
          class="relative min-w-32 overflow-hidden rounded-full"
          disabled={!canControlPlayback() || busy() !== null}
          onClick={() =>
            void runCommand(
              'pause',
              () => setPause(!(player()?.paused ?? true)),
              'Could not change playback state',
            )
          }
        >
          <span class="relative inline-grid h-5 w-5 place-items-center">
            <Play
              class={contextualIconClass(
                player()?.paused ?? true,
                'drop-shadow-[0_2px_4px_rgba(0,0,0,0.3)]',
              )}
            />
            <Pause
              class={contextualIconClass(
                !(player()?.paused ?? true),
                'drop-shadow-[0_2px_4px_rgba(0,0,0,0.3)]',
              )}
            />
          </span>
          <span class="font-bold tracking-wide">{player()?.paused ? 'Play' : 'Pause'}</span>
        </Button>
        <Button
          type="button"
          variant="icon"
          class="border-outline-variant/60 bg-surface-container-high/30 hover:border-error hover:text-error hover:bg-error/5 rounded-full border"
          aria-label="Stop playback"
          disabled={!canControlPlayback() || busy() !== null}
          onClick={() => void runCommand('stop', stopMpv, 'Could not stop MPV')}
        >
          <Square class="h-4 w-4 fill-current" />
        </Button>
        <Button
          type="button"
          variant="icon"
          class="border-outline-variant/60 bg-surface-container-high/30 hover:border-secondary hover:text-secondary hover:bg-secondary/5 rounded-full border"
          aria-label="Next episode"
          title={
            current()?.canPlayNext
              ? 'Next episode'
              : unavailableCopy(current()?.nextUnavailableReason)
          }
          disabled={!current()?.canPlayNext || busy() !== null}
          onClick={() => void runCommand('next', playNextEpisode, 'Could not play next episode')}
        >
          <SkipForward class="h-5 w-5" />
        </Button>
        <Show when={current()?.status === 'offline' && !connected()}>
          <Button
            type="button"
            variant="secondary"
            class="rounded-full"
            disabled={!props.jellyfinConnected || busy() !== null}
            onClick={() =>
              void runCommand('start', startMpv, 'Could not start MPV').then(() =>
                props.onPlayerStarted?.(),
              )
            }
            leadingIcon={<Play class="h-4.5 w-4.5 fill-current" />}
          >
            {props.jellyfinConnected ? 'Start MPV' : 'Reconnect Jellyfin first'}
          </Button>
        </Show>
      </div>

      <div
        class={`border-outline-variant bg-surface-container-lowest/50 relative grid gap-3 rounded-3xl border p-4 shadow-inner backdrop-blur-sm ${props.bare ? '' : 'sm:grid-cols-2'}`}
      >
        <JellyPilotSelect
          label="Audio"
          items={audioTrackItems()}
          value={selectedAudioTrackId()}
          placeholder="No audio tracks"
          disabled={!connected() || audioTrackItems().length === 0 || busy() !== null}
          size="compact"
          portalMount={props.trackSelectPortalMount}
          onValueChange={switchAudioTrack}
        />
        <JellyPilotSelect
          label="Subtitles"
          items={subtitleTrackItems()}
          value={selectedSubtitleTrackId()}
          placeholder="No subtitle tracks"
          disabled={!connected() || busy() !== null}
          size="compact"
          portalMount={props.trackSelectPortalMount}
          onValueChange={switchSubtitleTrack}
        />
      </div>

      <div
        class={`border-outline-variant bg-surface-container-lowest/50 relative flex items-center gap-3 rounded-3xl border p-4 shadow-inner backdrop-blur-sm ${props.bare ? '' : 'flex-col sm:flex-row'}`}
      >
        <Button
          type="button"
          variant="icon"
          class="hover:bg-secondary/15 hover:text-secondary hover:border-secondary/20 shrink-0 rounded-xl border border-transparent"
          aria-label={muted() ? 'Unmute' : 'Mute'}
          disabled={!connected() || busy() !== null}
          onClick={() => void runCommand('mute', toggleMute, 'Could not toggle mute')}
        >
          <span class="relative inline-grid h-5 w-5 place-items-center">
            <Volume2 class={contextualIconClass(connected() && !muted(), 'text-secondary')} />
            <VolumeX class={contextualIconClass(!connected() || muted(), 'text-error')} />
          </span>
        </Button>
        <Slider.Root
          aria-label={['Volume']}
          min={0}
          max={100}
          value={[volumeValue()]}
          disabled={!connected() || busy() !== null}
          onValueChange={(details) => setVolumeDraft(details.value[0] ?? 100)}
          onValueChangeEnd={(details) => commitVolume(details.value[0] ?? 100)}
          class="flex w-full flex-1 flex-col gap-2.5 disabled:opacity-50"
        >
          <Slider.Control class="relative flex h-10 cursor-pointer items-center">
            <Slider.Track class="bg-surface-container-highest/80 border-outline-variant/30 h-2.5 flex-1 overflow-hidden rounded-full border">
              <Slider.Range class="from-secondary to-primary h-full rounded-full bg-gradient-to-r shadow-[0_0_8px_rgba(129,140,248,0.4)] transition-[width,transform] duration-150" />
            </Slider.Track>
            <Slider.Thumb
              index={0}
              class="border-surface-container-lowest bg-on-surface data-[focus-visible]:ring-primary/50 flex h-5.5 w-5.5 cursor-grab items-center justify-center rounded-full border-2 shadow-lg shadow-black/50 transition-[box-shadow,transform] duration-200 outline-none hover:scale-110 hover:shadow-[0_0_12px_rgba(255,255,255,0.4)] active:cursor-grabbing data-[focus-visible]:ring-2"
            >
              <Slider.HiddenInput />
            </Slider.Thumb>
          </Slider.Control>
        </Slider.Root>
        <span class="text-secondary w-12 text-right font-mono text-[13px] font-semibold tabular-nums drop-shadow-[0_0_6px_rgba(129,140,248,0.15)]">
          {Math.round(volumeValue())}%
        </span>
      </div>
    </div>
  );

  if (props.bare) {
    return inner;
  }

  return (
    <Card
      as="section"
      variant="elevated"
      class="group/card relative space-y-6 overflow-hidden"
      aria-labelledby="now-playing-title"
    >
      {inner}
    </Card>
  );
}
