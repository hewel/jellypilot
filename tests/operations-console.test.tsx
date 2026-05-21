import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import { commands, events, type NowPlayingState } from '../src/bindings';
import OperationsConsole from '../src/components/OperationsConsole';
import { ToastProvider } from '../src/components/ToastProvider';

const connectedState = {
  connected: true,
  serverUrl: 'https://jellyfin.example.com',
  serverName: 'Jellyfin Home',
  userName: 'Ada',
};

const config = {
  deviceName: 'JMSR Test',
  mpvPath: null,
  mpvArgs: [],
  progressInterval: 5,
  startMinimized: false,
  introSkipperEnabled: true,
  keybindNext: 'Shift+n',
  keybindPrev: 'Shift+p',
};

const nowPlaying: NowPlayingState = {
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

function mockCommon() {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(connectedState);
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue(config);
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
}

function renderConsole(onSignedOut = () => undefined) {
  mockCommon();
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <OperationsConsole onSignedOut={onSignedOut} />
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
  localStorage.clear();
  document.body.innerHTML = '';
});

test('operations console loads intro skipper setting from config', async () => {
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByText('Operations Console')).toBeVisible(),
  );
  expect(screen.getByLabelText('Automatic Intro Skip')).toBeChecked();

  cleanup();
});

test('operations console saves changed intro skipper setting', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByDisplayValue('JMSR Test')).toBeVisible(),
  );
  const checkbox = screen.getByLabelText(
    'Automatic Intro Skip',
  ) as HTMLInputElement;
  fireEvent.click(checkbox);
  await waitFor(() => expect(checkbox).not.toBeChecked());
  fireEvent.click(screen.getByRole('button', { name: 'Save Settings' }));

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));
  expect(configSet).toHaveBeenCalledWith(
    expect.objectContaining({ introSkipperEnabled: false }),
  );
  await waitFor(() => expect(screen.getByText('Manual')).toBeVisible());

  cleanup();
});

test('disconnect keeps saved session and stays on console', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify({ serverUrl: 'x' }));
  const disconnect = rstest
    .spyOn(commands, 'jellyfinDisconnect')
    .mockResolvedValue({
      status: 'ok',
      data: null,
    });
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByText('Operations Console')).toBeVisible(),
  );
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Disconnect now' }),
    ).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Disconnect now' }));

  await waitFor(() => expect(disconnect).toHaveBeenCalledTimes(1));
  expect(localStorage.getItem('jmsr_auth_session')).not.toBeNull();
  expect(screen.getByText('Operations Console')).toBeVisible();

  cleanup();
});

test('sign out confirms and clears saved session', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify({ serverUrl: 'x' }));
  const clearSession = rstest
    .spyOn(commands, 'jellyfinClearSession')
    .mockResolvedValue({
      status: 'ok',
      data: null,
    });
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut);

  await waitFor(() =>
    expect(screen.getByText('Operations Console')).toBeVisible(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await waitFor(() => expect(screen.getByRole('dialog')).toBeVisible());
  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons[signOutButtons.length - 1]);

  await waitFor(() => expect(clearSession).toHaveBeenCalledTimes(1));
  expect(localStorage.getItem('jmsr_auth_session')).toBeNull();
  expect(onSignedOut).toHaveBeenCalledTimes(1);

  cleanup();
});
