import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor, within } from '@testing-library/dom';
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
  preferredSubtitleLanguages: [],
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

function mockCommon(appConfig = config) {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(connectedState);
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue(appConfig);
  rstest.spyOn(commands, 'nowPlayingGetState').mockResolvedValue({
    status: 'ok',
    data: nowPlaying,
  });
  rstest
    .spyOn(events.nowPlayingChanged, 'listen')
    .mockResolvedValue(() => undefined);
}

function renderConsole(onSignedOut = () => undefined, appConfig = config) {
  mockCommon(appConfig);
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

test('operations console autosaves changed intro skipper setting', async () => {
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

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));
  expect(configSet).toHaveBeenCalledWith(
    expect.objectContaining({ introSkipperEnabled: false }),
  );
  await waitFor(() => expect(screen.getByText('Manual')).toBeVisible());

  cleanup();
});

test('automatic intro skip tile toggles the synced checkbox optimistically', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const tile = screen.getByRole('button', { name: /Automatic Intro Skip/ });
  const checkbox = screen.getByLabelText(
    'Automatic Intro Skip',
  ) as HTMLInputElement;

  expect(tile).toHaveAttribute('aria-pressed', 'true');
  expect(checkbox).toBeChecked();

  fireEvent.click(tile);

  expect(tile).toHaveAttribute('aria-pressed', 'false');
  expect(checkbox).not.toBeChecked();
  expect(screen.getAllByText('Saving preference…').length).toBeGreaterThan(0);
  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(
      expect.objectContaining({ introSkipperEnabled: false }),
    ),
  );

  cleanup();
});

test('automatic intro skip rolls back and shows inline error on save failure', async () => {
  rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'error',
    error: { message: 'Config write failed' },
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const tile = screen.getByRole('button', { name: /Automatic Intro Skip/ });
  const checkbox = screen.getByLabelText(
    'Automatic Intro Skip',
  ) as HTMLInputElement;

  fireEvent.click(tile);

  expect(tile).toHaveAttribute('aria-pressed', 'false');
  await waitFor(() =>
    expect(screen.getAllByText('Config write failed').length).toBeGreaterThan(
      0,
    ),
  );
  expect(tile).toHaveAttribute('aria-pressed', 'true');
  expect(checkbox).toBeChecked();

  cleanup();
});

test('operations console autosaves compact preferred subtitle language chips', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole(() => undefined, {
    ...config,
    preferredSubtitleLanguages: ['jpn', 'eng'],
  });

  await waitFor(() =>
    expect(
      screen.getByRole('list', {
        name: 'Selected preferred subtitle languages',
      }),
    ).toBeVisible(),
  );
  const list = screen.getByRole('list', {
    name: 'Selected preferred subtitle languages',
  });
  expect(
    within(list)
      .getAllByText(/^(jpn|eng)$/)
      .map((el) => el.textContent),
  ).toEqual(['jpn', 'eng']);

  fireEvent.click(screen.getByRole('button', { name: 'Move jpn down' }));
  const input = screen.getByLabelText(
    'Add preferred subtitle language',
  ) as HTMLInputElement;
  fireEvent.input(input, { target: { value: ' SWE ' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  fireEvent.click(screen.getByRole('button', { name: 'Move swe up' }));
  fireEvent.input(input, { target: { value: 'spa' } });
  fireEvent.click(screen.getByRole('button', { name: 'Add language' }));
  fireEvent.click(screen.getByRole('button', { name: 'Remove jpn' }));

  await waitFor(() =>
    expect(configSet).toHaveBeenLastCalledWith(
      expect.objectContaining({
        preferredSubtitleLanguages: ['eng', 'swe', 'spa'],
      }),
    ),
  );
  expect(within(list).getByText('1')).toBeVisible();
  expect(
    within(list).getByRole('button', { name: 'Remove eng' }),
  ).toBeVisible();

  cleanup();
});
test('preferred subtitle language editor uses Ark tags input and combobox', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole();

  const input = await screen.findByLabelText('Add preferred subtitle language');
  expect(input.closest('[data-scope="tags-input"]')).not.toBeNull();

  fireEvent.input(input, { target: { value: 'jap' } });

  const suggestion = await screen.findByText('jpn — Japanese');
  expect(suggestion.closest('[data-scope="combobox"]')).not.toBeNull();
  fireEvent.click(suggestion);

  await waitFor(() =>
    expect(configSet).toHaveBeenLastCalledWith(
      expect.objectContaining({
        preferredSubtitleLanguages: ['jpn'],
      }),
    ),
  );
  expect(screen.getByRole('button', { name: 'Remove jpn' })).toBeVisible();

  cleanup();
});

test('operations console autosaves clearing preferred subtitle languages', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole(() => undefined, {
    ...config,
    preferredSubtitleLanguages: ['jpn'],
  });

  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Clear all' })).toBeVisible(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Clear all' }));
  expect(
    screen.getByText(
      /No preferred subtitle languages selected. JMSR will use Jellyfin and media defaults./,
    ),
  ).toBeVisible();

  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(
      expect.objectContaining({
        preferredSubtitleLanguages: [],
      }),
    ),
  );

  cleanup();
});

test('player bridge text fields autosave on valid blur and keep invalid drafts local', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole();

  const deviceName = (await screen.findByDisplayValue(
    'JMSR Test',
  )) as HTMLInputElement;
  fireEvent.input(deviceName, { target: { value: '' } });
  fireEvent.blur(deviceName);

  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(configSet).not.toHaveBeenCalled();

  const mpvPath = screen.getByPlaceholderText(
    'Path to mpv executable',
  ) as HTMLInputElement;
  fireEvent.input(mpvPath, { target: { value: '/usr/bin/mpv' } });
  fireEvent.blur(mpvPath);

  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(
      expect.objectContaining({
        deviceName: 'JMSR Test',
        mpvPath: '/usr/bin/mpv',
      }),
    ),
  );
  expect(screen.getByText('Saved')).toBeVisible();

  cleanup();
});

test('detect mpv autosaves detected path', async () => {
  rstest.spyOn(commands, 'configDetectMpv').mockResolvedValue('/opt/bin/mpv');
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  fireEvent.click(screen.getByRole('button', { name: 'Detect MPV' }));

  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(
      expect.objectContaining({ mpvPath: '/opt/bin/mpv' }),
    ),
  );

  cleanup();
});

test('autosaves are serialized without overwriting newer drafts', async () => {
  let resolveFirstSave: (() => void) | undefined;
  const configSet = rstest.spyOn(commands, 'configSet').mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveFirstSave = () => resolve({ status: 'ok' as const, data: null });
      }),
  );

  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const mpvPath = (await screen.findByPlaceholderText(
    'Path to mpv executable',
  )) as HTMLInputElement;
  fireEvent.input(mpvPath, { target: { value: '/one/mpv' } });
  fireEvent.blur(mpvPath);

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));

  const deviceName = screen.getByDisplayValue('JMSR Test') as HTMLInputElement;
  fireEvent.input(deviceName, { target: { value: 'JMSR Bridge' } });
  fireEvent.blur(deviceName);
  fireEvent.input(deviceName, { target: { value: 'JMSR Test' } });
  fireEvent.blur(deviceName);

  resolveFirstSave?.();

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(2));
  expect(configSet).toHaveBeenLastCalledWith(
    expect.objectContaining({
      deviceName: 'JMSR Test',
      mpvPath: '/one/mpv',
    }),
  );

  cleanup();
});

test('player bridge autosave failure recovers on later save', async () => {
  const configSet = rstest
    .spyOn(commands, 'configSet')
    .mockResolvedValueOnce({
      status: 'error',
      error: { message: 'Disk unavailable' },
    })
    .mockResolvedValueOnce({ status: 'ok', data: null });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const mpvPath = screen.getByPlaceholderText(
    'Path to mpv executable',
  ) as HTMLInputElement;
  fireEvent.input(mpvPath, { target: { value: '/broken/mpv' } });
  fireEvent.blur(mpvPath);

  await waitFor(() =>
    expect(screen.getAllByText('Disk unavailable').length).toBeGreaterThan(0),
  );

  const deviceName = screen.getByDisplayValue('JMSR Test') as HTMLInputElement;
  fireEvent.input(deviceName, { target: { value: 'JMSR Recovery' } });
  fireEvent.blur(deviceName);

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(2));
  expect(configSet).toHaveBeenLastCalledWith(
    expect.objectContaining({
      deviceName: 'JMSR Recovery',
      mpvPath: '/broken/mpv',
    }),
  );
  expect(screen.getByText('Saved')).toBeVisible();

  cleanup();
});

test('connection comes before now playing and hero keeps only refresh', async () => {
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByText('Operations Console')).toBeVisible(),
  );

  const headings = screen.getAllByRole('heading').map((heading) => ({
    text: heading.textContent,
    top: heading.getBoundingClientRect().top,
  }));
  expect(
    headings.findIndex((heading) => heading.text === 'Connection'),
  ).toBeLessThan(
    headings.findIndex(
      (heading) => heading.text === 'No active playback metadata',
    ),
  );
  expect(screen.getByRole('button', { name: 'Refresh status' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Start MPV' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Reconnect' })).toBeNull();

  cleanup();
});

test('final console structure covers all operational areas in order', async () => {
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const headings = screen
    .getAllByRole('heading')
    .map((heading) => heading.textContent);
  expect(headings).toEqual(
    expect.arrayContaining([
      'Connection',
      'No active playback metadata',
      'Player Bridge settings',
      'Diagnostics',
      'Automatic Intro Skip',
      'Session',
    ]),
  );
  expect(headings.indexOf('Connection')).toBeLessThan(
    headings.indexOf('No active playback metadata'),
  );
  expect(headings.indexOf('Diagnostics')).toBeLessThan(
    headings.indexOf('Automatic Intro Skip'),
  );
  expect(
    screen.getByRole('button', { name: 'Toggle diagnostics' }),
  ).toHaveAttribute('aria-expanded', 'false');

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
      screen.getByRole('button', { name: 'Disconnect' }),
    ).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));

  await waitFor(() => expect(disconnect).toHaveBeenCalledTimes(1));
  expect(localStorage.getItem('jmsr_auth_session')).not.toBeNull();
  expect(screen.getByText('Operations Console')).toBeVisible();
  expect(
    screen.getByText(
      /Disconnect ends the active Jellyfin connection but keeps the Saved Session available for Reconnect./,
    ),
  ).toBeVisible();

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
test('sign out dialog uses Ark dialog dismissal semantics', async () => {
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByText('Operations Console')).toBeVisible(),
  );

  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog).toBeVisible();
  expect(dialog.closest('[data-scope="dialog"]')).not.toBeNull();

  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  const escapeDialog = await screen.findByRole('dialog');
  fireEvent.keyDown(escapeDialog, { key: 'Escape', code: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await screen.findByRole('dialog');
  const backdrop = document.querySelector(
    '[data-scope="dialog"][data-part="backdrop"]',
  );
  expect(backdrop).not.toBeNull();
  fireEvent.pointerDown(backdrop as Element);
  fireEvent.click(backdrop as Element);
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  cleanup();
});

test('sign out dialog locks dismissal while signing out', async () => {
  let resolveClearSession:
    | ((
        result: Awaited<ReturnType<typeof commands.jellyfinClearSession>>,
      ) => void)
    | undefined;
  rstest.spyOn(commands, 'jellyfinClearSession').mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveClearSession = resolve;
      }),
  );
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByText('Operations Console')).toBeVisible(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await screen.findByRole('dialog');

  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons[signOutButtons.length - 1]);

  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled(),
  );
  fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.getByRole('dialog')).toBeVisible();

  resolveClearSession?.({ status: 'ok', data: null });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  cleanup();
});
test('player bridge settings use Ark field collapsible and checkbox primitives', async () => {
  const cleanup = renderConsole();

  const mpvPath = await screen.findByPlaceholderText('Path to mpv executable');
  expect(mpvPath.closest('[data-scope="field"]')).not.toBeNull();

  expect(
    screen.queryByPlaceholderText('--fullscreen&#10;--force-window'),
  ).toBeNull();

  const advancedTrigger = screen.getByRole('button', {
    name: 'Advanced MPV options',
  });
  expect(advancedTrigger.closest('[data-scope="collapsible"]')).not.toBeNull();

  fireEvent.click(advancedTrigger);
  await waitFor(() =>
    expect(advancedTrigger).toHaveAttribute('aria-expanded', 'true'),
  );
  const mpvArgs = await screen.findByLabelText('Extra arguments');
  expect(mpvArgs.closest('[data-scope="field"]')).not.toBeNull();
  expect(mpvArgs.closest('[data-scope="collapsible"]')).not.toBeNull();

  const introSkip = screen.getByRole('checkbox', {
    name: 'Automatic Intro Skip',
  });
  expect(introSkip.closest('[data-scope="checkbox"]')).not.toBeNull();

  cleanup();
});

test('settings and session actions keep shared visual semantics', async () => {
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const mpvPath = screen.getByPlaceholderText('Path to mpv executable');
  expect(mpvPath).toHaveClass('input-filled');
  expect(mpvPath.className).not.toMatch(/mpv/);

  const disconnect = screen.getByRole('button', { name: 'Disconnect' });
  expect(disconnect).toHaveClass('btn-outlined');
  expect(disconnect.className).not.toContain('border-error');

  const signOut = screen.getByRole('button', { name: 'Sign out' });
  expect(signOut.className).toContain('border-error');

  cleanup();
});
