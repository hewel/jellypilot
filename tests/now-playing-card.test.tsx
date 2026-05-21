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

function renderCard(state: NowPlayingState = offlineState) {
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
        <NowPlayingCard />
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

test('offline now playing disables transport controls and offers start mpv', async () => {
  const cleanup = renderCard();

  await waitFor(() =>
    expect(screen.getByText('Player bridge offline')).toBeVisible(),
  );
  expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Start MPV' })).toBeVisible();

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

test('next and previous are disabled when unavailable', async () => {
  const cleanup = renderCard();

  await waitFor(() =>
    expect(screen.getByLabelText('Next episode')).toBeDisabled(),
  );
  expect(screen.getByLabelText('Previous episode')).toBeDisabled();

  cleanup();
});
