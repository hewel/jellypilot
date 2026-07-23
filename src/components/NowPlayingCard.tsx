import { Slider } from '@ark-ui/solid/slider';
import { cx } from '@styled-system/css';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import { Exit, Match } from 'effect';
import {
  Info,
  MonitorOff,
  MonitorPlay,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Square,
  Volume2,
  VolumeX,
} from 'lucide-solid';
import { Show, createSignal } from 'solid-js';
import type { JSX } from 'solid-js';
import * as recipes from '~styles/recipes';

import type { NowPlayingState } from '../bindings';
import { commandFailureMessage } from '../effects/commands';
import {
  createNowPlayingState,
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
} from '../effects/nowPlaying';
import type { NowPlayingEffect } from '../effects/nowPlaying';
import { queryKeys, runExit } from '../effects/query';
import * as styles from './NowPlayingCard.styles';
import { useToast } from './ToastProvider';
import { Button, JellyPilotSelect, StatusBadge } from './ui';

function formatTime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const mins = Math.floor(total / 60);
  const secs = total % 60;
  return `${mins}:${secs.toString().padStart(2, '0')}`;
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

/** Shared Ark slider markup for seek and volume; owners supply scale + commit. */
function PlaybackSlider(props: {
  label: string;
  max: number;
  value: number;
  disabled: boolean;
  rangeClass: string;
  onDraft: (value: number) => void;
  onCommit: (value: number) => void;
}) {
  return (
    <Slider.Root
      aria-label={[props.label]}
      min={0}
      max={props.max}
      value={[props.value]}
      disabled={props.disabled}
      onValueChange={(details) => props.onDraft(details.value[0] ?? 0)}
      onValueChangeEnd={(details) => props.onCommit(details.value[0] ?? 0)}
      class={styles.sliderRoot}
    >
      <Slider.Control class={styles.sliderControl}>
        <Slider.Track class={styles.sliderTrack}>
          <Slider.Range class={cx(styles.sliderRange, props.rangeClass)} />
        </Slider.Track>
        <Slider.Thumb index={0} class={styles.thumb}>
          <Slider.HiddenInput />
        </Slider.Thumb>
      </Slider.Control>
    </Slider.Root>
  );
}

/** Non-playback state panel: icon, copy, and an optional single action. */
function StatePanel(props: {
  icon: JSX.Element;
  title: string;
  message: string;
  action?: JSX.Element;
}) {
  return (
    <div class={styles.statePanel}>
      <span class={styles.stateIcon}>{props.icon}</span>
      <p class={styles.stateTitle}>{props.title}</p>
      <p class={styles.stateMessage}>{props.message}</p>
      <Show when={props.action}>{(action) => action()}</Show>
    </div>
  );
}

export default function NowPlayingCard(props: {
  jellyfinConnected: boolean;
  trackSelectPortalMount?: HTMLElement;
}) {
  const { showToast } = useToast();
  const queryClient = useQueryClient();
  const [busy, setBusy] = createSignal<string | null>(null);
  const [seekDraft, setSeekDraft] = createSignal<number | null>(null);
  const [volumeDraft, setVolumeDraft] = createSignal<number | null>(null);
  const { query: nowPlayingQuery, state: current } = createNowPlayingState({
    onExternalChange: (state) => {
      setSeekDraft(null);
      setVolumeDraft(null);
      void queryClient.invalidateQueries({
        queryKey: queryKeys.mpvTracks(state.player.connected),
      });
    },
  });
  const player = () => current()?.player;
  const connected = () => player()?.connected ?? false;
  const status = () => current()?.status ?? 'unknown';
  const canControlPlayback = () => {
    const value = status();
    return value === 'playing' || value === 'paused';
  };
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

  const seekValue = () => seekDraft() ?? player()?.timePos ?? 0;
  const volumeValue = () => volumeDraft() ?? player()?.volume ?? 100;
  const muted = () => player()?.muted ?? false;
  const activeTimeline = () => {
    const duration = player()?.duration ?? 0;
    return connected() && Number.isFinite(duration) && duration > 0;
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
  const audioTrackItems = () =>
    tracks()
      .filter((track) => track.type === 'audio')
      .map((track) => ({ label: track.label, value: track.id.toString() }));
  const subtitleTrackItems = () => [
    { label: 'Off', value: '-1' },
    ...tracks()
      .filter((track) => track.type === 'sub')
      .map((track) => ({ label: track.label, value: track.id.toString() })),
  ];
  const selectedAudioTrackId = () =>
    tracks()
      .find((track) => track.type === 'audio' && track.selected)
      ?.id.toString() ?? null;
  const selectedSubtitleTrackId = () =>
    tracks()
      .find((track) => track.type === 'sub' && track.selected)
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
    if (!activeTimeline() || busy() !== null) {
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

  return (
    <div class={styles.root}>
      <div class={styles.header}>
        <div class={styles.headerCopy}>
          <div class={styles.titleRow}>
            <h2 id="now-playing-title" class={styles.title}>
              {mediaTitle()}
            </h2>
            <Show when={status() === 'playing'}>
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
          <StatusBadge variant={statusVariant(status())}>{statusLabel(status())}</StatusBadge>
        </div>
      </div>

      <Show when={canControlPlayback()}>
        <div class={styles.panel}>
          <div class={styles.timeRow}>
            <span>{formatTime(seekValue())}</span>
            <span>
              {activeTimeline() ? formatTime(player()?.duration ?? 0) : 'Timeline unavailable'}
            </span>
          </div>
          <Show when={activeTimeline()}>
            <PlaybackSlider
              label="Seek position"
              max={player()?.duration ?? 0}
              value={seekValue()}
              disabled={busy() !== null}
              rangeClass={styles.primaryRange}
              onDraft={setSeekDraft}
              onCommit={commitSeek}
            />
          </Show>
        </div>

        <div class={styles.controls}>
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
            disabled={busy() !== null}
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
            disabled={busy() !== null}
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
        </div>

        <div class={styles.selectPanel}>
          <JellyPilotSelect
            label="Audio"
            items={audioTrackItems()}
            value={selectedAudioTrackId()}
            placeholder="No audio tracks"
            disabled={audioTrackItems().length === 0 || busy() !== null}
            size="compact"
            portalMount={props.trackSelectPortalMount}
            onValueChange={switchAudioTrack}
          />
          <JellyPilotSelect
            label="Subtitles"
            items={subtitleTrackItems()}
            value={selectedSubtitleTrackId()}
            placeholder="No subtitle tracks"
            disabled={busy() !== null}
            size="compact"
            portalMount={props.trackSelectPortalMount}
            onValueChange={switchSubtitleTrack}
          />
        </div>
      </Show>

      <Show when={status() === 'idle'}>
        <StatePanel
          icon={<MonitorPlay />}
          title="MPV is idle"
          message="Awaiting playback command from Jellyfin."
        />
      </Show>
      <Show when={status() === 'offline'}>
        <StatePanel
          icon={<MonitorOff />}
          title="Player is offline"
          message={
            props.jellyfinConnected
              ? 'Start MPV to take control of playback from Jellyfin.'
              : 'Reconnect Jellyfin, then start MPV.'
          }
          action={
            <Button
              type="button"
              variant="primary"
              class={recipes.pillButton}
              disabled={!props.jellyfinConnected || busy() !== null}
              onClick={() => void runCommand('start', startMpv, 'Could not start MPV')}
              leadingIcon={<Play class={styles.playIcon} />}
            >
              {props.jellyfinConnected ? 'Start MPV' : 'Reconnect Jellyfin first'}
            </Button>
          }
        />
      </Show>
      <Show when={status() === 'unknown'}>
        <StatePanel
          icon={<Info />}
          title="Playback state unknown"
          message="Controls will appear when MPV reports its state."
        />
      </Show>

      <Show when={connected()}>
        <div class={styles.volumePanel}>
          <Button
            type="button"
            variant="icon"
            class={styles.muteButton}
            aria-label={muted() ? 'Unmute' : 'Mute'}
            disabled={busy() !== null}
            onClick={() => void runCommand('mute', toggleMute, 'Could not toggle mute')}
          >
            <span class={styles.iconSlot}>
              <Volume2
                class={cx(styles.contextualIcon({ visible: !muted() }), styles.secondaryIcon)}
              />
              <VolumeX class={cx(styles.contextualIcon({ visible: muted() }), styles.errorIcon)} />
            </span>
          </Button>
          <PlaybackSlider
            label="Volume"
            max={100}
            value={volumeValue()}
            disabled={busy() !== null}
            rangeClass={styles.secondaryRange}
            onDraft={setVolumeDraft}
            onCommit={commitVolume}
          />
          <span class={styles.volumeValue}>{Math.round(volumeValue())}%</span>
        </div>
      </Show>
    </div>
  );
}
