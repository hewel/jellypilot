import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor, within } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import type { AppConfig, SavedSession } from '../src/bindings';
import OperationsConsole from '../src/components/OperationsConsole';
import { ToastProvider } from '../src/components/ToastProvider';

const connectedState = {
  connected: true,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userName: 'Ada',
};

const config: AppConfig = {
  deviceName: 'JMSR Test',
  introSkipperMode: 'automatic',
  keybindIntroSkip: 'g',
  keybindNext: 'Shift+>',
  keybindPrev: 'Shift+<',
  mpvArgs: [],
  mpvPath: null,
  preferredSubtitleLanguages: [],
  progressInterval: 5,
  startMinimized: false,
};

const validSavedSession: SavedSession = {
  accessToken: 'token-1',
  deviceId: 'device-1',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

function mockCommon(appConfig = config, state = connectedState) {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue(state);
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue(appConfig);
}

function renderConsole(onSignedOut = () => {}, appConfig = config, state = connectedState) {
  mockCommon(appConfig, state);
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
test('operations console reports config load command failures', async () => {
  mockCommon();
  rstest.spyOn(commands, 'configGet').mockRejectedValue(new Error('Config unavailable'));
  const consoleError = rstest.spyOn(console, 'error').mockImplementation(() => {});
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <OperationsConsole onSignedOut={() => {}} />
      </ToastProvider>
    ),
    root,
  );

  await waitFor(() =>
    expect(consoleError).toHaveBeenCalledWith('Failed to load config:', 'Config unavailable'),
  );

  dispose();
  root.remove();
});

test('operations console loads intro skipper mode from config', async () => {
  const cleanup = renderConsole();

  await screen.findByRole('heading', { name: 'Connection' });
  expect(screen.getByRole('button', { name: /Automatic/ })).toHaveAttribute('aria-pressed', 'true');

  cleanup();
});

test('operations console autosaves changed intro skipper mode', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await waitFor(() => expect(screen.getByDisplayValue('JMSR Test')).toBeVisible());
  fireEvent.click(screen.getByRole('button', { name: /Manual/ }));

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));
  expect(configSet).toHaveBeenCalledWith(expect.objectContaining({ introSkipperMode: 'manual' }));

  cleanup();
});

test('intro skip mode toggles optimistically', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const automatic = screen.getByRole('button', { name: /Automatic/ });
  const manual = screen.getByRole('button', { name: /Manual/ });

  expect(automatic).toHaveAttribute('aria-pressed', 'true');
  expect(manual).toHaveAttribute('aria-pressed', 'false');

  fireEvent.click(manual);

  expect(automatic).toHaveAttribute('aria-pressed', 'false');
  expect(manual).toHaveAttribute('aria-pressed', 'true');
  expect(screen.getAllByText('Saving preference…').length).toBeGreaterThan(0);
  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(expect.objectContaining({ introSkipperMode: 'manual' })),
  );

  cleanup();
});

test('intro skip mode rolls back and shows inline error on save failure', async () => {
  rstest.spyOn(commands, 'configSet').mockResolvedValue({
    error: { code: 'internal', message: 'Config write failed' },
    status: 'error',
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const automatic = screen.getByRole('button', { name: /Automatic/ });
  const manual = screen.getByRole('button', { name: /Manual/ });

  fireEvent.click(manual);

  expect(manual).toHaveAttribute('aria-pressed', 'true');
  await waitFor(() => expect(screen.getAllByText('Config write failed').length).toBeGreaterThan(0));
  expect(automatic).toHaveAttribute('aria-pressed', 'true');
  expect(manual).toHaveAttribute('aria-pressed', 'false');

  cleanup();
});
test('intro skip mode reports rejected save commands through status and toast', async () => {
  rstest.spyOn(commands, 'configSet').mockRejectedValue(new Error('Disk unavailable'));
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  fireEvent.click(screen.getByRole('button', { name: /Manual/ }));

  await waitFor(() => expect(screen.getAllByText('Disk unavailable').length).toBeGreaterThan(0));

  cleanup();
});

test('operations console autosaves changed intro skip key', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  expect(screen.getByRole('heading', { name: 'Shortcut keys' })).toBeVisible();
  const key = screen.getByDisplayValue('g');
  fireEvent.input(key, { target: { value: 'i' } });
  fireEvent.blur(key);

  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(expect.objectContaining({ keybindIntroSkip: 'i' })),
  );

  cleanup();
});

test('operations console autosaves compact preferred subtitle language chips', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole(() => {}, {
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
  const input = screen.getByLabelText('Custom subtitle language code') as HTMLInputElement;
  fireEvent.input(input, { target: { value: ' SWE ' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  fireEvent.click(screen.getByRole('button', { name: 'Move swe up' }));
  fireEvent.input(input, { target: { value: 'spa' } });
  fireEvent.click(screen.getByRole('button', { name: 'Add' }));
  fireEvent.click(screen.getByRole('button', { name: 'Remove jpn' }));

  await waitFor(() =>
    expect(configSet).toHaveBeenLastCalledWith(
      expect.objectContaining({
        preferredSubtitleLanguages: ['eng', 'swe', 'spa'],
      }),
    ),
  );
  expect(within(list).getByText('1')).toBeVisible();
  expect(within(list).getByRole('button', { name: 'Remove eng' })).toBeVisible();

  cleanup();
});
test('preferred subtitle language editor uses Ark tags input and select', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();
  await screen.findByDisplayValue('JMSR Test');

  // The custom code input lives inside the tags-input scope
  const customInput = await screen.findByLabelText('Custom subtitle language code');
  expect(customInput.closest('[data-scope="tags-input"]')).not.toBeNull();

  // The Ark Select trigger is rendered with a combobox role
  const selectTrigger = await screen.findByRole('combobox', {
    name: 'Predefined languages',
  });
  expect(selectTrigger.closest('[data-scope="select"]')).not.toBeNull();

  // The select trigger shows a placeholder
  expect(selectTrigger).toHaveTextContent('Select a language…');
  // The select offers predefined language options via its collection
  const selectRoot = selectTrigger.closest('[data-scope="select"]');
  expect(selectRoot).not.toBeNull();

  // Custom code entry via text input
  fireEvent.input(customInput, { target: { value: 'jpn' } });
  fireEvent.keyDown(customInput, { key: 'Enter' });

  await waitFor(() =>
    expect(configSet).toHaveBeenLastCalledWith(
      expect.objectContaining({
        preferredSubtitleLanguages: ['jpn'],
      }),
    ),
  );
  expect(screen.getByRole('button', { name: 'Remove jpn' })).toBeVisible();

  // Add another custom code
  fireEvent.input(customInput, { target: { value: 'tha' } });
  fireEvent.keyDown(customInput, { key: 'Enter' });

  await waitFor(() =>
    expect(configSet).toHaveBeenLastCalledWith(
      expect.objectContaining({
        preferredSubtitleLanguages: ['jpn', 'tha'],
      }),
    ),
  );
  expect(screen.getByRole('button', { name: 'Remove tha' })).toBeVisible();

  // The Add button has rounded corners consistent with the input field (rounded-2xl)
  const addButton = screen.getByRole('button', { name: 'Add' });
  expect(addButton.className).toContain('rounded-2xl');
  expect(addButton.className).toContain('h-14');

  cleanup();
});

test('operations console autosaves clearing preferred subtitle languages', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole(() => {}, {
    ...config,
    preferredSubtitleLanguages: ['jpn'],
  });

  await waitFor(() => expect(screen.getByRole('button', { name: 'Clear all' })).toBeVisible());
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
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  const deviceName = (await screen.findByDisplayValue('JMSR Test')) as HTMLInputElement;
  fireEvent.input(deviceName, { target: { value: '' } });
  fireEvent.blur(deviceName);

  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(configSet).not.toHaveBeenCalled();

  const mpvPath = screen.getByPlaceholderText('Path to mpv executable') as HTMLInputElement;
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
  await waitFor(() => expect(screen.getByText('Saved')).toBeVisible());

  cleanup();
});

test('detect mpv autosaves detected path', async () => {
  rstest.spyOn(commands, 'configDetectMpv').mockResolvedValue('/opt/bin/mpv');
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  fireEvent.click(screen.getByRole('button', { name: 'Detect MPV' }));

  await waitFor(() =>
    expect(configSet).toHaveBeenCalledWith(expect.objectContaining({ mpvPath: '/opt/bin/mpv' })),
  );

  cleanup();
});

test('autosaves are serialized without overwriting newer drafts', async () => {
  let resolveFirstSave: (() => void) | undefined;
  const configSet = rstest.spyOn(commands, 'configSet').mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveFirstSave = () => resolve({ data: null, status: 'ok' as const });
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
      error: { code: 'internal', message: 'Disk unavailable' },
      status: 'error',
    })
    .mockResolvedValueOnce({ data: null, status: 'ok' });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const mpvPath = screen.getByPlaceholderText('Path to mpv executable') as HTMLInputElement;
  fireEvent.input(mpvPath, { target: { value: '/broken/mpv' } });
  fireEvent.blur(mpvPath);

  await waitFor(() => expect(screen.getAllByText('Disk unavailable').length).toBeGreaterThan(0));

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
  await waitFor(() => expect(screen.getByText('Saved')).toBeVisible());

  cleanup();
});

test('connection comes before player settings and hero keeps only refresh', async () => {
  const cleanup = renderConsole();

  await screen.findByRole('heading', { name: 'Connection' });

  const headings = screen.getAllByRole('heading').map((heading) => ({
    text: heading.textContent,
    top: heading.getBoundingClientRect().top,
  }));
  expect(headings.findIndex((heading) => heading.text === 'Connection')).toBeLessThan(
    headings.findIndex((heading) => heading.text === 'Player Bridge settings'),
  );
  expect(screen.getByRole('button', { name: 'Refresh status' })).toBeVisible();
  expect(screen.queryByRole('button', { name: 'Start MPV' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Reconnect' })).toBeNull();

  cleanup();
});

test('final console structure covers all operational areas in order', async () => {
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const headings = screen.getAllByRole('heading').map((heading) => heading.textContent);
  expect(headings).toEqual(
    expect.arrayContaining([
      'Connection',
      'Player Bridge settings',
      'Diagnostics',
      'Intro Skip',
      'Session',
    ]),
  );
  expect(headings.indexOf('Diagnostics')).toBeLessThan(headings.indexOf('Intro Skip'));
  expect(screen.getByRole('button', { name: 'Toggle diagnostics' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );

  cleanup();
});

test('disconnect keeps saved session and stays on console', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify({ serverUrl: 'x' }));
  const disconnect = rstest.spyOn(commands, 'jellyfinDisconnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await screen.findByRole('heading', { name: 'Connection' });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Disconnect' })).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));

  await waitFor(() => expect(disconnect).toHaveBeenCalledTimes(1));
  expect(localStorage.getItem('jmsr_auth_session')).not.toBeNull();
  expect(screen.getByRole('heading', { name: 'Connection' })).toBeVisible();
  expect(
    screen.getByText(
      /Disconnect ends the active Jellyfin connection but keeps the Saved Session available for Reconnect./,
    ),
  ).toBeVisible();

  cleanup();
});
test('disconnect failure stays on console and unlocks the action', async () => {
  rstest.spyOn(commands, 'jellyfinDisconnect').mockResolvedValue({
    error: { code: 'network', message: 'disconnect offline' },
    status: 'error',
  });
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Disconnect' })).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));

  await waitFor(() => expect(screen.getByText('disconnect offline')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Disconnect' })).not.toBeDisabled();
  expect(screen.getByRole('heading', { name: 'Connection' })).toBeVisible();

  cleanup();
});

test('disconnect rejected commands stay on console and unlock the action', async () => {
  rstest
    .spyOn(commands, 'jellyfinDisconnect')
    .mockRejectedValue(new Error('disconnect ipc unavailable'));
  const cleanup = renderConsole();

  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Disconnect' })).not.toBeDisabled(),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));

  await waitFor(() => expect(screen.getByText('disconnect ipc unavailable')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Disconnect' })).not.toBeDisabled();
  expect(screen.getByRole('heading', { name: 'Connection' })).toBeVisible();

  cleanup();
});

test('reconnect restores a live Jellyfin connection from a Saved Session', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify(validSavedSession));
  const restore = rstest.spyOn(commands, 'jellyfinRestoreSession').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole(() => {}, config, {
    connected: false,
    serverName: null,
    serverUrl: null,
    userName: null,
  });

  await waitFor(() => expect(screen.getByRole('button', { name: 'Reconnect' })).toBeVisible());
  fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));

  await waitFor(() => expect(restore).toHaveBeenCalledWith(validSavedSession));
  expect(localStorage.getItem('jmsr_auth_session')).not.toBeNull();

  cleanup();
});

test('reconnect failure clears the Saved Session and signs out', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify(validSavedSession));
  rstest.spyOn(commands, 'jellyfinRestoreSession').mockResolvedValue({
    error: { code: 'authFailed', message: 'expired' },
    status: 'error',
  });
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut, config, {
    connected: false,
    serverName: null,
    serverUrl: null,
    userName: null,
  });

  await waitFor(() => expect(screen.getByRole('button', { name: 'Reconnect' })).toBeVisible());
  fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));

  await waitFor(() => expect(onSignedOut).toHaveBeenCalledTimes(1));
  expect(localStorage.getItem('jmsr_auth_session')).toBeNull();

  cleanup();
});

test('sign out confirms and clears saved session', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify({ serverUrl: 'x' }));
  const clearSession = rstest.spyOn(commands, 'jellyfinClearSession').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut);

  await screen.findByRole('heading', { name: 'Connection' });
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await waitFor(() => expect(screen.getByRole('dialog')).toBeVisible());
  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons.at(-1));

  await waitFor(() => {
    expect(clearSession).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem('jmsr_auth_session')).toBeNull();
    expect(onSignedOut).toHaveBeenCalledTimes(1);
  });

  cleanup();
});

test('sign out failure preserves the Saved Session and stays on console', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify(validSavedSession));
  const clearSession = rstest.spyOn(commands, 'jellyfinClearSession').mockResolvedValue({
    error: { code: 'network', message: 'offline' },
    status: 'error',
  });
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut);

  await screen.findByRole('heading', { name: 'Connection' });
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await waitFor(() => expect(screen.getByRole('dialog')).toBeVisible());
  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons.at(-1));

  await waitFor(() => expect(clearSession).toHaveBeenCalledTimes(1));
  expect(localStorage.getItem('jmsr_auth_session')).not.toBeNull();
  expect(onSignedOut).not.toHaveBeenCalled();
  expect(screen.getByRole('heading', { name: 'Connection' })).toBeVisible();

  cleanup();
});
test('sign out rejected commands preserve the Saved Session and close the dialog', async () => {
  localStorage.setItem('jmsr_auth_session', JSON.stringify(validSavedSession));
  rstest
    .spyOn(commands, 'jellyfinClearSession')
    .mockRejectedValue(new Error('sign out ipc unavailable'));
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut);

  await screen.findByRole('heading', { name: 'Connection' });
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await waitFor(() => expect(screen.getByRole('dialog')).toBeVisible());
  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons.at(-1));

  await waitFor(() => expect(screen.getByText('sign out ipc unavailable')).toBeVisible());
  expect(localStorage.getItem('jmsr_auth_session')).not.toBeNull();
  expect(onSignedOut).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  cleanup();
});
test('sign out dialog uses Ark dialog dismissal semantics', async () => {
  const cleanup = renderConsole();

  await screen.findByRole('heading', { name: 'Connection' });

  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  const dialog = await screen.findByRole('dialog');
  expect(dialog).toBeVisible();
  expect(dialog.closest('[data-scope="dialog"]')).not.toBeNull();

  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  const escapeDialog = await screen.findByRole('dialog');
  fireEvent.keyDown(escapeDialog, { code: 'Escape', key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await screen.findByRole('dialog');
  const backdrop = document.querySelector('[data-scope="dialog"][data-part="backdrop"]');
  expect(backdrop).not.toBeNull();
  fireEvent.pointerDown(backdrop as Element);
  fireEvent.click(backdrop as Element);
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  cleanup();
});

test('sign out dialog locks dismissal while signing out', async () => {
  let resolveClearSession:
    | ((result: Awaited<ReturnType<typeof commands.jellyfinClearSession>>) => void)
    | undefined;
  rstest.spyOn(commands, 'jellyfinClearSession').mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveClearSession = resolve;
      }),
  );
  const cleanup = renderConsole();

  await screen.findByRole('heading', { name: 'Connection' });
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await screen.findByRole('dialog');

  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons.at(-1));

  await waitFor(() => expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled());
  fireEvent.keyDown(document, { key: 'Escape' });
  expect(screen.getByRole('dialog')).toBeVisible();

  resolveClearSession?.({ data: null, status: 'ok' });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  cleanup();
});
test('player bridge settings use Ark fields and intro skip mode buttons', async () => {
  const cleanup = renderConsole();

  const mpvPath = await screen.findByPlaceholderText('Path to mpv executable');
  expect(mpvPath.closest('[data-scope="field"]')).not.toBeNull();

  expect(screen.queryByPlaceholderText('--fullscreen&#10;--force-window')).toBeNull();

  const advancedTrigger = screen.getByRole('button', {
    name: 'Advanced MPV options',
  });
  expect(advancedTrigger.closest('[data-scope="collapsible"]')).not.toBeNull();

  fireEvent.click(advancedTrigger);
  await waitFor(() => expect(advancedTrigger).toHaveAttribute('aria-expanded', 'true'));
  const mpvArgs = await screen.findByLabelText('Extra arguments');
  expect(mpvArgs.closest('[data-scope="field"]')).not.toBeNull();
  expect(mpvArgs.closest('[data-scope="collapsible"]')).not.toBeNull();

  const shortcutHeading = screen.getByRole('heading', {
    name: 'Shortcut keys',
  });
  const shortcutGroup = shortcutHeading.closest('aside');
  if (shortcutGroup === null) {
    throw new Error('Shortcut keys aside should render');
  }
  expect(shortcutHeading.closest('[data-scope="collapsible"]')).toBeNull();
  const shortcutFields = within(shortcutGroup);
  expect(shortcutFields.getByText('Next episode key')).toBeVisible();
  expect(shortcutFields.getByText('Previous episode key')).toBeVisible();
  const nextKey = shortcutFields.getByDisplayValue('Shift+>');
  const previousKey = shortcutFields.getByDisplayValue('Shift+<');
  expect(nextKey).toHaveValue('Shift+>');
  expect(nextKey).toHaveAttribute('placeholder', 'Shift+>');
  expect(previousKey).toHaveValue('Shift+<');
  expect(previousKey).toHaveAttribute('placeholder', 'Shift+<');
  const manual = screen.getByRole('button', { name: /Manual/ });
  expect(manual).toHaveAttribute('aria-pressed', 'false');

  cleanup();
});

test('settings and session actions keep shared visual semantics', async () => {
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JMSR Test');
  const mpvPath = screen.getByPlaceholderText('Path to mpv executable');
  expect(mpvPath).toBeVisible();
  expect(mpvPath.className).toContain('border-outline-variant/80');
  expect(mpvPath.className).not.toContain('input-filled');
  expect(mpvPath.className).not.toMatch(/mpv/);

  const disconnect = screen.getByRole('button', { name: 'Disconnect' });
  expect(disconnect).toBeVisible();
  expect(disconnect).toBeEnabled();
  expect(disconnect.className).not.toContain('border-error');

  const signOut = screen.getByRole('button', { name: 'Sign out' });
  expect(signOut.className).toContain('border-error');

  cleanup();
});

import { Cause, Effect, Exit } from 'effect';

import {
  clearSavedSession,
  loadSavedSession,
  SESSION_STORAGE_KEY,
  saveSession,
} from '../src/effects/auth';
import { StorageParseError } from '../src/effects/errors';

const sampleSession = {
  accessToken: 'token-1',
  deviceId: 'device-1',
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

test('session save and load round-trips through Effect', () => {
  Effect.runSync(saveSession(sampleSession));
  const exit = Effect.runSyncExit(loadSavedSession());
  expect(Exit.isSuccess(exit)).toBe(true);
  if (Exit.isSuccess(exit)) {
    expect(exit.value).toEqual(sampleSession);
  }
});

test('clearSavedSession removes the stored session', () => {
  Effect.runSync(saveSession(sampleSession));
  Effect.runSync(clearSavedSession());
  const exit = Effect.runSyncExit(loadSavedSession());
  expect(Exit.isSuccess(exit)).toBe(true);
  if (Exit.isSuccess(exit)) {
    expect(exit.value).toBeNull();
  }
});

test('loadSavedSession returns StorageParseError for malformed JSON', () => {
  localStorage.setItem(SESSION_STORAGE_KEY, '{bad');
  const exit = Effect.runSyncExit(loadSavedSession());
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const reason = exit.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed StorageParseError failure');
    }
    const { error } = reason;
    expect(error).toBeInstanceOf(StorageParseError);
    expect(error.key).toBe(SESSION_STORAGE_KEY);
  }
});

test('loadSavedSession returns null when no session is stored', () => {
  const exit = Effect.runSyncExit(loadSavedSession());
  expect(Exit.isSuccess(exit)).toBe(true);
  if (Exit.isSuccess(exit)) {
    expect(exit.value).toBeNull();
  }
});
