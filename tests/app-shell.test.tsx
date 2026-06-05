import { afterEach, expect, rstest, test } from '@rstest/core';
import { screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import { commands, events, type NowPlayingState } from '../src/bindings';
import AuthenticatedShell from '../src/components/AuthenticatedShell';
import { ToastProvider } from '../src/components/ToastProvider';

const connectedState = {
  connected: true,
  serverUrl: 'https://jellyfin.example.com',
  serverName: 'Jellyfin Home',
  userName: 'Ada',
};

const disconnectedState = {
  ...connectedState,
  connected: false,
};

const nowPlaying: NowPlayingState = {
  status: 'playing',
  player: {
    connected: true,
    paused: false,
    muted: false,
    timePos: 42,
    duration: 180,
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
  canPlayPrevious: false,
  nextUnavailableReason: null,
  previousUnavailableReason: 'noCurrentItem',
};

function mockShellCommands(state = connectedState) {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(state);
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
}

function renderShell(
  activeArea:
    | 'library'
    | 'now-playing'
    | 'settings'
    | 'diagnostics' = 'library',
) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <AuthenticatedShell
          activeArea={activeArea}
          onSignedOut={() => undefined}
        />
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

test('authenticated shell exposes peer navigation areas', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });

  const nav = screen.getByRole('navigation', { name: 'JMSR areas' });
  const libraryLink = screen.getByRole('link', { name: 'Library' });
  expect(nav).toBeVisible();
  expect(nav).toHaveClass('overflow-x-auto');
  expect(nav).toHaveClass('lg:flex-col');
  expect(libraryLink).toHaveAttribute('aria-current', 'page');
  expect(libraryLink).toHaveClass('focus-visible:ring-2');
  expect(screen.getByRole('link', { name: 'Now Playing' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Settings' })).toBeVisible();
  expect(screen.getByRole('link', { name: 'Diagnostics' })).toBeVisible();

  cleanup();
});

test('library landing renders connected placeholder and compact now playing link', async () => {
  mockShellCommands();
  const cleanup = renderShell();

  await screen.findByRole('heading', { name: 'Library' });

  expect(await screen.findByText('Video Home is ready')).toBeVisible();
  expect(screen.getByText('The Pilot')).toBeVisible();
  expect(
    screen.getByRole('link', { name: 'Open Now Playing' }),
  ).toHaveAttribute('href', '/now-playing');

  cleanup();
});

test('library landing exposes disconnected and retry states', async () => {
  mockShellCommands(disconnectedState);
  const cleanup = renderShell();

  await screen.findByText('Library requires a live Jellyfin connection');
  expect(screen.getByRole('button', { name: 'Retry Library' })).toBeVisible();

  cleanup();
});

test('library landing surfaces command errors without fake content', async () => {
  rstest
    .spyOn(commands, 'jellyfinGetState')
    .mockRejectedValue(new Error('IPC unavailable'));
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
  const cleanup = renderShell();

  await screen.findByText('Could not load Library state');
  expect(screen.getByRole('button', { name: 'Retry Library' })).toBeVisible();
  expect(screen.queryByText('Continue Watching')).toBeNull();

  cleanup();
});

test('now playing area exposes full playback controls', async () => {
  mockShellCommands();
  const cleanup = renderShell('now-playing');

  await waitFor(() => expect(screen.getByText('The Pilot')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Pause' })).toBeVisible();
  expect(screen.getByRole('slider', { name: 'Seek position' })).toBeVisible();

  cleanup();
});
