import { UIRoot } from '@jellypilot/ui';
import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands, events } from '../src/bindings';
import type { NowPlayingState } from '../src/bindings';
import NowPlayingCard from '../src/components/NowPlayingCard';
import { ToastProvider } from '../src/components/ToastProvider';
import { TestQueryProvider } from './query-client';

Element.prototype.scrollTo = () => {};

const offlineState: NowPlayingState = {
  canPlayNext: false,
  canPlayPrevious: false,
  media: null,
  nextUnavailableReason: 'noCurrentItem',
  player: {
    connected: false,
    duration: 0,
    muted: false,
    paused: true,
    timePos: 0,
    volume: 100,
  },
  previousUnavailableReason: 'noCurrentItem',
  status: 'offline',
};

const playingState: NowPlayingState = {
  canPlayNext: true,
  canPlayPrevious: true,
  media: {
    episodeNumber: 1,
    itemId: 'episode-1',
    itemType: 'Episode',
    name: 'The Pilot',
    seasonNumber: 1,
    seriesName: 'Example Show',
  },
  nextUnavailableReason: null,
  player: {
    connected: true,
    duration: 120,
    muted: false,
    paused: false,
    timePos: 30,
    volume: 80,
  },
  previousUnavailableReason: null,
  status: 'playing',
};

const idleState: NowPlayingState = {
  ...offlineState,
  player: {
    ...offlineState.player,
    connected: true,
  },
  status: 'idle',
};

const unknownState: NowPlayingState = {
  ...offlineState,
  status: 'unknown',
};

const pausedWithoutMetadataState: NowPlayingState = {
  ...playingState,
  media: null,
  player: {
    ...playingState.player,
    paused: true,
  },
  status: 'paused',
};

const trackList = JSON.stringify([
  { id: 1, lang: 'eng', selected: true, title: 'English Stereo', type: 'audio' },
  { id: 2, lang: 'jpn', selected: false, title: 'Japanese 5.1', type: 'audio' },
  { id: 3, lang: 'eng', selected: true, title: 'English Subtitles', type: 'sub' },
]);

function renderCard(state: NowPlayingState = offlineState, jellyfinConnected = true) {
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({ data: state, status: 'ok' });
  rstest.spyOn(commands, 'mpvGetProperty').mockResolvedValue({ data: trackList, status: 'ok' });
  rstest.spyOn(events.nowPlayingChanged, 'listen').mockResolvedValue(() => {});
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <UIRoot>
        <TestQueryProvider>
          <ToastProvider>
            <NowPlayingCard jellyfinConnected={jellyfinConnected} />
          </ToastProvider>
        </TestQueryProvider>
      </UIRoot>
    ),
    root,
  );
  return () => {
    dispose();
    root.remove();
  };
}

afterEach(() => {
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});

test('offline now playing offers start mpv when Jellyfin is connected', async () => {
  const cleanup = renderCard();

  await waitFor(() => expect(screen.getByText('Offline')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Start MPV' })).toBeVisible();

  cleanup();
});

test('media controls use shared icon buttons and primary text action', async () => {
  const cleanup = renderCard();

  await waitFor(() => expect(screen.getByText('Offline')).toBeVisible());

  const previous = screen.getByLabelText('Previous episode');
  expect(previous).toBeVisible();
  expect(previous).toBeDisabled();
  const stop = screen.getByLabelText('Stop playback');
  expect(stop).toBeVisible();
  expect(stop).toBeDisabled();
  const startMpv = screen.getByRole('button', { name: 'Start MPV' });
  expect(startMpv).toHaveTextContent('Start MPV');
  expect(startMpv.querySelector('svg')).not.toBeNull();

  cleanup();
});

test('offline now playing blocks start mpv when Jellyfin is disconnected', async () => {
  const startMpv = rstest
    .spyOn(commands, 'mpvStart')
    .mockResolvedValue({ data: null, status: 'ok' });
  const cleanup = renderCard(offlineState, false);

  await waitFor(() => expect(screen.getByText('Offline')).toBeVisible());

  expect(screen.getByText('Reconnect Jellyfin before starting MPV')).toBeVisible();
  const button = screen.getByRole('button', {
    name: 'Reconnect Jellyfin first',
  });
  expect(button).toBeDisabled();
  fireEvent.click(button);
  expect(startMpv).not.toHaveBeenCalled();

  cleanup();
});

test('playing state exposes transport controls and media metadata', async () => {
  const setPause = rstest
    .spyOn(commands, 'mpvSetPause')
    .mockResolvedValue({ data: null, status: 'ok' });
  const cleanup = renderCard(playingState);

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());
  expect(screen.getByText('Example Show · S01E01')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Pause' }));

  await waitFor(() => expect(setPause).toHaveBeenCalledWith(true));
  cleanup();
});
test('playing state exposes public seek and volume sliders', async () => {
  const seek = rstest.spyOn(commands, 'mpvSeek').mockResolvedValue({ data: null, status: 'ok' });
  const setVolume = rstest
    .spyOn(commands, 'mpvSetVolume')
    .mockResolvedValue({ data: null, status: 'ok' });
  const cleanup = renderCard(playingState);

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());

  const seekSlider = screen.getByRole('slider', { name: 'Seek position' });
  const volumeSlider = screen.getByRole('slider', { name: 'Volume' });

  expect(seekSlider.closest('[data-ui="slider"]')).not.toBeNull();
  expect(volumeSlider.closest('[data-ui="slider"]')).not.toBeNull();

  expect(seekSlider).toHaveAttribute('aria-valuemin', '0');
  expect(seekSlider).toHaveAttribute('aria-valuemax', '120');
  expect(volumeSlider).toHaveAttribute('aria-valuemin', '0');
  expect(volumeSlider).toHaveAttribute('aria-valuemax', '100');
  expect(seek).not.toHaveBeenCalled();
  expect(setVolume).not.toHaveBeenCalled();

  cleanup();
});

test('playing state exposes audio and subtitle selectors', async () => {
  const setAudioTrack = rstest
    .spyOn(commands, 'mpvSetAudioTrack')
    .mockResolvedValue({ data: null, status: 'ok' });
  const setSubtitleTrack = rstest
    .spyOn(commands, 'mpvSetSubtitleTrack')
    .mockResolvedValue({ data: null, status: 'ok' });
  const cleanup = renderCard(playingState);

  const audio = await screen.findByRole('button', { name: 'Audio' });
  const subtitles = await screen.findByRole('button', { name: 'Subtitles' });
  await waitFor(() => expect(audio).toHaveTextContent('English Stereo'));
  await waitFor(() => expect(subtitles).toHaveTextContent('English Subtitles'));

  fireEvent.click(audio);
  fireEvent.click(await screen.findByRole('option', { name: 'Japanese 5.1' }));
  await waitFor(() => expect(setAudioTrack).toHaveBeenCalledWith(2));

  await waitFor(() => expect(screen.getByRole('button', { name: 'Subtitles' })).not.toBeDisabled());
  fireEvent.click(screen.getByRole('button', { name: 'Subtitles' }));
  fireEvent.click(await screen.findByRole('option', { name: 'Off' }));
  await waitFor(() => expect(setSubtitleTrack).toHaveBeenCalledWith(-1));

  cleanup();
});

test('next and previous are disabled when unavailable', async () => {
  const cleanup = renderCard();

  await waitFor(() => expect(screen.getByLabelText('Next episode')).toBeDisabled());
  expect(screen.getByLabelText('Previous episode')).toBeDisabled();

  cleanup();
});

test('idle and unknown states disable transport controls without exposing startup', async () => {
  const cleanup = renderCard(idleState);

  await waitFor(() => expect(screen.getByText('MPV idle')).toBeVisible());
  await waitFor(() => expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled());
  expect(screen.getByLabelText('Stop playback')).toBeDisabled();
  expect(screen.queryByRole('button', { name: 'Start MPV' })).toBeNull();
  cleanup();

  const cleanupUnknown = renderCard(unknownState);
  await waitFor(() => expect(screen.getByText('Unknown')).toBeVisible());
  await waitFor(() => expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled());
  expect(screen.getByLabelText('Stop playback')).toBeDisabled();
  expect(screen.queryByRole('button', { name: 'Start MPV' })).toBeNull();

  cleanupUnknown();
});

test('paused playback remains controllable without metadata', async () => {
  const setPause = rstest
    .spyOn(commands, 'mpvSetPause')
    .mockResolvedValue({ data: null, status: 'ok' });
  const cleanup = renderCard(pausedWithoutMetadataState);

  await waitFor(() => expect(screen.getByText('Paused')).toBeVisible());

  expect(screen.getByRole('button', { name: 'Play' })).not.toBeDisabled();
  expect(screen.getByLabelText('Stop playback')).not.toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Play' }));

  await waitFor(() => expect(setPause).toHaveBeenCalledWith(false));
  cleanup();
});

test('clicking mute toggles icon and label after state reloads', async () => {
  const nowPlayingGetState = rstest
    .spyOn(commands, 'nowPlayingGetState')
    .mockResolvedValue({ data: playingState, status: 'ok' });
  const toggleMute = rstest.spyOn(commands, 'mpvToggleMute').mockImplementation(async () => {
    nowPlayingGetState.mockResolvedValue({
      data: {
        ...playingState,
        player: { ...playingState.player, muted: true },
      },
      status: 'ok',
    });
    return { data: null, status: 'ok' };
  });
  rstest.spyOn(events.nowPlayingChanged, 'listen').mockResolvedValue(() => {});
  rstest.spyOn(commands, 'mpvGetProperty').mockResolvedValue({ data: trackList, status: 'ok' });
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <UIRoot>
        <TestQueryProvider>
          <ToastProvider>
            <NowPlayingCard jellyfinConnected />
          </ToastProvider>
        </TestQueryProvider>
      </UIRoot>
    ),
    root,
  );

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());

  expect(screen.getByLabelText('Mute')).toBeVisible();
  const muteBtn = screen.getByRole('button', { name: 'Mute' });
  fireEvent.click(muteBtn);

  await waitFor(() => expect(toggleMute).toHaveBeenCalled());
  await waitFor(() => expect(screen.getByRole('button', { name: 'Unmute' })).toBeVisible());

  dispose();
  root.remove();
});
