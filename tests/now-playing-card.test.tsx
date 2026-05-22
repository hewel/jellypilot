import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import { commands, events, type NowPlayingState } from '../src/bindings';
import NowPlayingCard from '../src/components/NowPlayingCard';
import { ToastProvider } from '../src/components/ToastProvider';

const offlineState: NowPlayingState = {
  status: 'offline',
  player: {
    connected: false,
    paused: true,
    timePos: 0,
    duration: 0,
    volume: 100,
  },
  media: null,
  canPlayNext: false,
  canPlayPrevious: false,
  nextUnavailableReason: 'noCurrentItem',
  previousUnavailableReason: 'noCurrentItem',
};

const playingState: NowPlayingState = {
  status: 'playing',
  player: {
    connected: true,
    paused: false,
    timePos: 30,
    duration: 120,
    volume: 80,
  },
  media: {
    itemId: 'episode-1',
    name: 'The Pilot',
    itemType: 'Episode',
    seriesName: 'Example Show',
    seasonNumber: 1,
    episodeNumber: 1,
  },
  canPlayNext: true,
  canPlayPrevious: true,
  nextUnavailableReason: null,
  previousUnavailableReason: null,
};

const idleState: NowPlayingState = {
  ...offlineState,
  status: 'idle',
  player: {
    ...offlineState.player,
    connected: true,
  },
};

const unknownState: NowPlayingState = {
  ...offlineState,
  status: 'unknown',
};

const pausedWithoutMetadataState: NowPlayingState = {
  ...playingState,
  status: 'paused',
  player: {
    ...playingState.player,
    paused: true,
  },
  media: null,
};

function renderCard(
  state: NowPlayingState = offlineState,
  jellyfinConnected = true,
) {
  rstest
    .spyOn(commands, 'nowPlayingGetState')
    .mockResolvedValue({ status: 'ok', data: state });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <NowPlayingCard jellyfinConnected={jellyfinConnected} />
      </ToastProvider>
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

  await waitFor(() =>
    expect(screen.getByText('Player bridge offline')).toBeVisible(),
  );
  expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Start MPV' })).toBeVisible();

  cleanup();
});

test('media controls use shared icon buttons and primary text action', async () => {
  const cleanup = renderCard();

  await waitFor(() =>
    expect(screen.getByText('Player bridge offline')).toBeVisible(),
  );

  expect(screen.getByLabelText('Previous episode')).toHaveClass('btn-icon');
  expect(screen.getByLabelText('Stop playback')).toHaveClass('btn-icon');
  const startMpv = screen.getByRole('button', { name: 'Start MPV' });
  expect(startMpv).toHaveTextContent('Start MPV');
  expect(startMpv.querySelector('svg')).not.toBeNull();

  cleanup();
});

test('offline now playing blocks start mpv when Jellyfin is disconnected', async () => {
  const startMpv = rstest
    .spyOn(commands, 'mpvStart')
    .mockResolvedValue({ status: 'ok', data: null });
  const cleanup = renderCard(offlineState, false);

  await waitFor(() =>
    expect(screen.getByText('Player bridge offline')).toBeVisible(),
  );

  expect(
    screen.getByText('Reconnect Jellyfin before starting MPV'),
  ).toBeVisible();
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
    .mockResolvedValue({ status: 'ok', data: null });
  const cleanup = renderCard(playingState);

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());
  expect(screen.getByText('Example Show · S01E01')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Pause' }));

  await waitFor(() => expect(setPause).toHaveBeenCalledWith(true));
  cleanup();
});
test('playing state uses Ark sliders for seek and volume', async () => {
  const seek = rstest
    .spyOn(commands, 'mpvSeek')
    .mockResolvedValue({ status: 'ok', data: null });
  const setVolume = rstest
    .spyOn(commands, 'mpvSetVolume')
    .mockResolvedValue({ status: 'ok', data: null });
  const cleanup = renderCard(playingState);

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());

  const seekSlider = screen.getByRole('slider', { name: 'Seek position' });
  const volumeSlider = screen.getByRole('slider', { name: 'Volume' });

  expect(seekSlider.closest('[data-scope="slider"]')).not.toBeNull();
  expect(volumeSlider.closest('[data-scope="slider"]')).not.toBeNull();

  expect(seekSlider).toHaveAttribute('aria-valuemin', '0');
  expect(seekSlider).toHaveAttribute('aria-valuemax', '120');
  expect(volumeSlider).toHaveAttribute('aria-valuemin', '0');
  expect(volumeSlider).toHaveAttribute('aria-valuemax', '100');
  expect(seek).not.toHaveBeenCalled();
  expect(setVolume).not.toHaveBeenCalled();

  cleanup();
});

test('next and previous are disabled when unavailable', async () => {
  const cleanup = renderCard();

  await waitFor(() =>
    expect(screen.getByLabelText('Next episode')).toBeDisabled(),
  );
  expect(screen.getByLabelText('Previous episode')).toBeDisabled();

  cleanup();
});

test('idle and unknown states disable transport controls without exposing startup', async () => {
  const cleanup = renderCard(idleState);

  await waitFor(() => expect(screen.getByText('MPV idle')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
  expect(screen.getByLabelText('Stop playback')).toBeDisabled();
  expect(screen.queryByRole('button', { name: 'Start MPV' })).toBeNull();
  cleanup();

  const cleanupUnknown = renderCard(unknownState);
  await waitFor(() =>
    expect(screen.getByText('Playback state unknown')).toBeVisible(),
  );
  expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
  expect(screen.getByLabelText('Stop playback')).toBeDisabled();
  expect(screen.queryByRole('button', { name: 'Start MPV' })).toBeNull();

  cleanupUnknown();
});

test('paused playback remains controllable without metadata', async () => {
  const setPause = rstest
    .spyOn(commands, 'mpvSetPause')
    .mockResolvedValue({ status: 'ok', data: null });
  const cleanup = renderCard(pausedWithoutMetadataState);

  await waitFor(() => expect(screen.getByText('Paused')).toBeVisible());

  expect(screen.getByRole('button', { name: 'Play' })).not.toBeDisabled();
  expect(screen.getByLabelText('Stop playback')).not.toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'Play' }));

  await waitFor(() => expect(setPause).toHaveBeenCalledWith(false));
  cleanup();
});
