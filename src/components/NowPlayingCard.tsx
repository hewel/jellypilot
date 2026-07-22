import { Slider } from '@ark-ui/solid/slider';
import { cx } from '@styled-system/css';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Match } from 'effect';
import { Pause, Play, SkipBack, SkipForward, Square, Volume2, VolumeX } from 'lucide-solid';
import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import * as recipes from '~styles/recipes';

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
import * as styles from './NowPlayingCard.styles';
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
    queryFn: () => runExit(fetchNowPlayingState),
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
    mutationFn: (command: NowPlayingEffect<void>) => runExit(command),
  }));
  const tracks = () =>
    tracksQuery.data && Exit.isSuccess(tracksQuery.data) ? tracksQuery.data.value : [];

  const runCommand = async (key: string, command: NowPlayingEffect<void>, failure: string) => {
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
    void runCommand('audio-track', setAudioTrack(id), 'Could not switch audio track');
  };
  const switchSubtitleTrack = (value: string) => {
    const id = Number(value);
    if (value.length === 0 || !Number.isFinite(id) || busy() !== null) {
      return;
    }
    void runCommand('subtitle-track', setSubtitleTrack(id), 'Could not switch subtitle track');
  };
  const commitSeek = (value: number) => {
    if (!activeTimeline() || !canControlPlayback() || busy() !== null) {
      return;
    }
    void runCommand('seek', seekPlayback(value), 'Could not seek playback');
  };

  const commitVolume = (value: number) => {
    if (!connected() || busy() !== null) {
      return;
    }
    void runCommand('volume', setVolume(value), 'Could not set volume');
  };

  const inner = (
    <div class={styles.root({ bare: props.bare ?? false })}>
      <div class={styles.header}>
        <div class={styles.headerCopy({ bare: props.bare ?? false })}>
          {!props.bare && <p class={styles.eyebrow}>Now Playing</p>}
          <div class={styles.titleRow}>
            <h2 id="now-playing-title" class={styles.title({ bare: props.bare ?? false })}>
              {mediaTitle()}
            </h2>
            <Show when={current()?.status === 'playing'}>
              <div class={styles.equalizer} aria-hidden="true" title="Playing stream">
                <span class={cx(styles.waveBar, styles.wavePrimary, styles.waveTiming.a)} />
                <span class={cx(styles.waveBar, styles.waveSecondary, styles.waveTiming.b)} />
                <span class={cx(styles.waveBar, styles.wavePrimary, styles.waveTiming.c)} />
                <span class={cx(styles.waveBar, styles.waveSecondary, styles.waveTiming.d)} />
              </div>
            </Show>
          </div>
          <p class={styles.subtitle}>{mediaSubtitle()}</p>
        </div>
        <div class={styles.badgePlacement}>
          <StatusBadge variant={statusVariant(current()?.status ?? 'unknown')}>
            {statusLabel(current()?.status ?? 'unknown')}
          </StatusBadge>
        </div>
      </div>

      <div class={styles.panel}>
        <div class={styles.timeRow}>
          <span>{formatTime(seekValue())}</span>
          <span>
            {activeTimeline() ? formatTime(player()?.duration ?? 0) : 'Timeline unavailable'}
          </span>
        </div>
        <Show when={activeTimeline()} fallback={<div class={styles.emptyTrack} />}>
          <Slider.Root
            aria-label={['Seek position']}
            min={0}
            max={player()?.duration ?? 0}
            value={[seekValue()]}
            disabled={!activeTimeline() || !canControlPlayback() || busy() !== null}
            onValueChange={(details) => setSeekDraft(details.value[0] ?? 0)}
            onValueChangeEnd={(details) => commitSeek(details.value[0] ?? 0)}
            class={styles.sliderRoot}
          >
            <Slider.Control class={styles.sliderControl}>
              <Slider.Track class={styles.sliderTrack}>
                <Slider.Range class={cx(styles.sliderRange, styles.primaryRange)} />
              </Slider.Track>
              <Slider.Thumb index={0} class={styles.thumb}>
                <Slider.HiddenInput />
              </Slider.Thumb>
            </Slider.Control>
          </Slider.Root>
        </Show>
      </div>

      <div class={styles.controls({ bare: props.bare ?? false })}>
        <Button
          type="button"
          variant="icon"
          class={styles.iconButton}
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
          <SkipBack class={styles.icon5} />
        </Button>
        <Button
          type="button"
          variant="primary"
          class={styles.playPauseButton}
          disabled={!canControlPlayback() || busy() !== null}
          onClick={() =>
            void runCommand(
              'pause',
              setPause(!(player()?.paused ?? true)),
              'Could not change playback state',
            )
          }
        >
          <span class={styles.iconSlot}>
            <Play
              class={cx(
                styles.contextualIcon({ visible: player()?.paused ?? true }),
                styles.iconDropShadow,
              )}
            />
            <Pause
              class={cx(
                styles.contextualIcon({ visible: !(player()?.paused ?? true) }),
                styles.iconDropShadow,
              )}
            />
          </span>
          <span class={styles.actionLabel}>{player()?.paused ? 'Play' : 'Pause'}</span>
        </Button>
        <Button
          type="button"
          variant="icon"
          class={cx(styles.iconButton, styles.stopButton)}
          aria-label="Stop playback"
          disabled={!canControlPlayback() || busy() !== null}
          onClick={() => void runCommand('stop', stopMpv, 'Could not stop MPV')}
        >
          <Square class={styles.squareIcon} />
        </Button>
        <Button
          type="button"
          variant="icon"
          class={styles.iconButton}
          aria-label="Next episode"
          title={
            current()?.canPlayNext
              ? 'Next episode'
              : unavailableCopy(current()?.nextUnavailableReason)
          }
          disabled={!current()?.canPlayNext || busy() !== null}
          onClick={() => void runCommand('next', playNextEpisode, 'Could not play next episode')}
        >
          <SkipForward class={styles.icon5} />
        </Button>
        <Show when={current()?.status === 'offline' && !connected()}>
          <Button
            type="button"
            variant="secondary"
            class={recipes.pillButton}
            disabled={!props.jellyfinConnected || busy() !== null}
            onClick={() =>
              void runCommand('start', startMpv, 'Could not start MPV').then(() =>
                props.onPlayerStarted?.(),
              )
            }
            leadingIcon={<Play class={styles.playIcon} />}
          >
            {props.jellyfinConnected ? 'Start MPV' : 'Reconnect Jellyfin first'}
          </Button>
        </Show>
      </div>

      <div class={styles.selectPanel}>
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

      <div class={styles.volumePanel}>
        <Button
          type="button"
          variant="icon"
          class={styles.muteButton}
          aria-label={muted() ? 'Unmute' : 'Mute'}
          disabled={!connected() || busy() !== null}
          onClick={() => void runCommand('mute', toggleMute, 'Could not toggle mute')}
        >
          <span class={styles.iconSlot}>
            <Volume2
              class={cx(
                styles.contextualIcon({ visible: connected() && !muted() }),
                styles.secondaryIcon,
              )}
            />
            <VolumeX
              class={cx(
                styles.contextualIcon({ visible: !connected() || muted() }),
                styles.errorIcon,
              )}
            />
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
          class={styles.sliderRoot}
        >
          <Slider.Control class={styles.sliderControl}>
            <Slider.Track class={styles.sliderTrack}>
              <Slider.Range class={cx(styles.sliderRange, styles.secondaryRange)} />
            </Slider.Track>
            <Slider.Thumb index={0} class={styles.thumb}>
              <Slider.HiddenInput />
            </Slider.Thumb>
          </Slider.Control>
        </Slider.Root>
        <span class={styles.volumeValue}>{Math.round(volumeValue())}%</span>
      </div>
    </div>
  );

  if (props.bare) {
    return inner;
  }

  return (
    <Card as="section" variant="elevated" class={styles.card} aria-labelledby="now-playing-title">
      {inner}
    </Card>
  );
}
