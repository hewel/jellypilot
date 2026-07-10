import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor, within } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import type { AppConfig, SavedServiceProfiles } from '../src/bindings';
import OperationsConsole from '../src/components/OperationsConsole';
import { ToastProvider } from '../src/components/ToastProvider';
import { ConfigCoordinatorProvider } from '../src/effects/configContext';
import { TestQueryProvider } from './query-client';

const connectedState = {
  capabilities: {
    introSkipper: true,
    quickConnect: true,
    remoteControl: true,
    remoteControlAvailable: true,
    remoteControlWarning: null,
  },
  connected: true,
  provider: 'jellyfin' as const,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

const embyConnectedState = {
  ...connectedState,
  capabilities: {
    introSkipper: false,
    quickConnect: false,
    remoteControl: true,
    remoteControlAvailable: false,
    remoteControlWarning: null,
  },
  provider: 'emby' as const,
  serverName: 'Emby Home',
  serverUrl: 'https://media.example.com/emby',
  userId: 'emby-user-1',
};

const config: AppConfig = {
  deviceName: 'JellyPilot Test',
  imageDiskCacheEnabled: true,
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

const validSavedProfiles: SavedServiceProfiles = {
  activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
  profiles: [
    {
      active: true,
      key: 'jellyfin|https://jellyfin.example.com|Ada',
      lastRestoreError: null,
      provider: 'jellyfin',
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
  ],
};

const embySavedProfile = {
  active: false,
  key: 'emby|https://media.example.com/emby|Ada',
  lastRestoreError: null,
  provider: 'emby' as const,
  serverName: 'Emby Home',
  serverUrl: 'https://media.example.com/emby',
  userName: 'Ada',
};

const multipleSavedProfiles: SavedServiceProfiles = {
  activeProfileKey: validSavedProfiles.activeProfileKey,
  profiles: [...validSavedProfiles.profiles, embySavedProfile],
};

function mockCommon(
  appConfig = config,
  state = connectedState,
  savedProfiles: SavedServiceProfiles = validSavedProfiles,
) {
  rstest.spyOn(commands, 'serverGetState').mockResolvedValue(state);
  rstest.spyOn(commands, 'serverProfilesGet').mockResolvedValue({
    data: savedProfiles,
    status: 'ok',
  });
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue(appConfig);
}

function renderConsole(
  onSignedOut = () => {},
  appConfig = config,
  state = connectedState,
  savedProfiles: SavedServiceProfiles = validSavedProfiles,
) {
  mockCommon(appConfig, state, savedProfiles);
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ConfigCoordinatorProvider>
        <TestQueryProvider>
          <ToastProvider>
            <OperationsConsole onSignedOut={onSignedOut} />
          </ToastProvider>
        </TestQueryProvider>
      </ConfigCoordinatorProvider>
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
      <ConfigCoordinatorProvider>
        <TestQueryProvider>
          <ToastProvider>
            <OperationsConsole onSignedOut={() => {}} />
          </ToastProvider>
        </TestQueryProvider>
      </ConfigCoordinatorProvider>
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

test('operations console renders Emby capabilities without Intro Skipper controls', async () => {
  const cleanup = renderConsole(() => {}, config, embyConnectedState);

  expect(await screen.findByText('Emby Home')).toBeVisible();
  expect(screen.getByText('Pending')).toBeVisible();
  expect(screen.queryByRole('heading', { name: 'Intro Skip' })).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Intro skip key')).not.toBeInTheDocument();
  expect(screen.getByLabelText('Next episode key')).toBeVisible();

  cleanup();
});

test('operations console autosaves changed intro skipper mode', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await waitFor(() => expect(screen.getByDisplayValue('JellyPilot Test')).toBeVisible());
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

  await screen.findByDisplayValue('JellyPilot Test');
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

  await screen.findByDisplayValue('JellyPilot Test');
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

  await screen.findByDisplayValue('JellyPilot Test');
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

  await screen.findByDisplayValue('JellyPilot Test');
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
  await screen.findByDisplayValue('JellyPilot Test');

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
      /No preferred subtitle languages selected. JellyPilot will use Jellyfin and media defaults./,
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

  const deviceName = (await screen.findByDisplayValue('JellyPilot Test')) as HTMLInputElement;
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
        deviceName: 'JellyPilot Test',
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

  await screen.findByDisplayValue('JellyPilot Test');
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

  await screen.findByDisplayValue('JellyPilot Test');
  const mpvPath = (await screen.findByPlaceholderText(
    'Path to mpv executable',
  )) as HTMLInputElement;
  fireEvent.input(mpvPath, { target: { value: '/one/mpv' } });
  fireEvent.blur(mpvPath);

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));

  const deviceName = screen.getByDisplayValue('JellyPilot Test') as HTMLInputElement;
  fireEvent.input(deviceName, { target: { value: 'JellyPilot Bridge' } });
  fireEvent.blur(deviceName);
  fireEvent.input(deviceName, { target: { value: 'JellyPilot Test' } });
  fireEvent.blur(deviceName);

  resolveFirstSave?.();

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(2));
  expect(configSet).toHaveBeenLastCalledWith(
    expect.objectContaining({
      deviceName: 'JellyPilot Test',
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

  await screen.findByDisplayValue('JellyPilot Test');
  const mpvPath = screen.getByPlaceholderText('Path to mpv executable') as HTMLInputElement;
  fireEvent.input(mpvPath, { target: { value: '/broken/mpv' } });
  fireEvent.blur(mpvPath);

  await waitFor(() => expect(screen.getAllByText('Disk unavailable').length).toBeGreaterThan(0));
  await waitFor(() => expect(mpvPath).toHaveValue(''));

  const deviceName = screen.getByDisplayValue('JellyPilot Test') as HTMLInputElement;
  fireEvent.input(deviceName, { target: { value: 'JellyPilot Recovery' } });
  fireEvent.blur(deviceName);

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(2));
  expect(configSet).toHaveBeenLastCalledWith(
    expect.objectContaining({
      deviceName: 'JellyPilot Recovery',
      mpvPath: null,
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

  await screen.findByDisplayValue('JellyPilot Test');
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

test('disconnect keeps saved services and stays on console', async () => {
  localStorage.setItem('jellypilot_auth_session', JSON.stringify({ serverUrl: 'x' }));
  const disconnect = rstest.spyOn(commands, 'serverDisconnect').mockResolvedValue({
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
  expect(localStorage.getItem('jellypilot_auth_session')).not.toBeNull();
  expect(screen.getByRole('heading', { name: 'Connection' })).toBeVisible();
  expect(
    screen.getByText(
      /Disconnect ends the active media server connection but keeps saved services available for Reconnect./,
    ),
  ).toBeVisible();

  cleanup();
});
test('disconnect failure stays on console and unlocks the action', async () => {
  rstest.spyOn(commands, 'serverDisconnect').mockResolvedValue({
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
    .spyOn(commands, 'serverDisconnect')
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

test('reconnect activates the active saved service profile', async () => {
  const activate = rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    data: validSavedProfiles,
    status: 'ok',
  });
  const cleanup = renderConsole(() => {}, config, {
    ...connectedState,
    connected: false,
    serverName: null,
    serverUrl: null,
    userId: null,
    userName: null,
  });

  await waitFor(() => expect(screen.getByRole('button', { name: 'Reconnect' })).toBeVisible());
  fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));

  await waitFor(() => expect(activate).toHaveBeenCalledWith(validSavedProfiles.activeProfileKey));

  cleanup();
});

test('reconnect failure keeps the saved service profile available', async () => {
  rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    error: { code: 'authFailed', message: 'expired' },
    status: 'error',
  });
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut, config, {
    ...connectedState,
    connected: false,
    serverName: null,
    serverUrl: null,
    userId: null,
    userName: null,
  });

  await waitFor(() => expect(screen.getByRole('button', { name: 'Reconnect' })).toBeVisible());
  fireEvent.click(screen.getByRole('button', { name: 'Reconnect' }));

  await waitFor(() =>
    expect(screen.getByText('Could not reconnect to the saved service.')).toBeVisible(),
  );
  expect(onSignedOut).not.toHaveBeenCalled();
  expect(screen.getByText('Jellyfin Home')).toBeVisible();

  cleanup();
});

test('saved services card activates an inactive profile', async () => {
  const activate = rstest.spyOn(commands, 'serverProfilesActivate').mockResolvedValue({
    data: {
      activeProfileKey: embySavedProfile.key,
      profiles: [
        { ...validSavedProfiles.profiles[0], active: false },
        { ...embySavedProfile, active: true },
      ],
    },
    status: 'ok',
  });
  const cleanup = renderConsole(() => {}, config, connectedState, multipleSavedProfiles);

  await screen.findByText('Emby Home');
  fireEvent.click(screen.getByRole('button', { name: 'Activate' }));

  await waitFor(() => expect(activate).toHaveBeenCalledWith(embySavedProfile.key));

  cleanup();
});

test('add service dialog accepts embedded login form text input', async () => {
  const cleanup = renderConsole();

  await screen.findByText('Saved Services');
  fireEvent.click(screen.getByRole('button', { name: 'Add service' }));

  const addService = await screen.findByRole('dialog', { name: 'Add saved service' });
  fireEvent.click(within(addService).getByRole('radio', { name: 'Password' }));
  const host = within(addService).getByPlaceholderText(
    'jellyfin.local or media.example.com/jellyfin',
  );
  const username = await within(addService).findByPlaceholderText('Jellyfin username');
  const password = await within(addService).findByPlaceholderText('Jellyfin password');

  fireEvent.input(host, {
    currentTarget: { value: 'emby.local' },
    target: { value: 'emby.local' },
  });
  fireEvent.input(username, { currentTarget: { value: 'Ada' }, target: { value: 'Ada' } });
  fireEvent.input(password, {
    currentTarget: { value: 'secret' },
    target: { value: 'secret' },
  });

  expect(host).toHaveValue('emby.local');
  expect(username).toHaveValue('Ada');
  expect(password).toHaveValue('secret');

  cleanup();
});

test('sign out confirms and removes the active saved service profile', async () => {
  const removeProfile = rstest.spyOn(commands, 'serverProfilesRemove').mockResolvedValue({
    data: { activeProfileKey: null, profiles: [] },
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
    expect(removeProfile).toHaveBeenCalledWith(validSavedProfiles.activeProfileKey);
    expect(onSignedOut).toHaveBeenCalledTimes(1);
  });

  cleanup();
});

test('sign out failure preserves the active saved service profile and stays on console', async () => {
  const removeProfile = rstest.spyOn(commands, 'serverProfilesRemove').mockResolvedValue({
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

  await waitFor(() => expect(removeProfile).toHaveBeenCalledTimes(1));
  expect(onSignedOut).not.toHaveBeenCalled();
  await waitFor(() => expect(screen.getByText('offline')).toBeVisible());
  expect(screen.getAllByText('Jellyfin Home').length).toBeGreaterThan(0);
  expect(screen.getByRole('heading', { name: 'Connection' })).toBeVisible();

  cleanup();
});
test('sign out rejected commands preserve the saved service profile and close the dialog', async () => {
  rstest
    .spyOn(commands, 'serverProfilesRemove')
    .mockRejectedValue(new Error('sign out ipc unavailable'));
  const onSignedOut = rstest.fn();
  const cleanup = renderConsole(onSignedOut);

  await screen.findByRole('heading', { name: 'Connection' });
  fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
  await waitFor(() => expect(screen.getByRole('dialog')).toBeVisible());
  const signOutButtons = screen.getAllByRole('button', { name: 'Sign out' });
  fireEvent.click(signOutButtons.at(-1));

  await waitFor(() => expect(screen.getByText('sign out ipc unavailable')).toBeVisible());
  expect(onSignedOut).not.toHaveBeenCalled();
  expect(screen.getAllByText('Jellyfin Home').length).toBeGreaterThan(0);
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
  let resolveRemoveProfile:
    | ((result: Awaited<ReturnType<typeof commands.serverProfilesRemove>>) => void)
    | undefined;
  rstest.spyOn(commands, 'serverProfilesRemove').mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveRemoveProfile = resolve;
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

  resolveRemoveProfile?.({ data: { activeProfileKey: null, profiles: [] }, status: 'ok' });
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

test('settings and session actions render usable controls', async () => {
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JellyPilot Test');
  const mpvPath = screen.getByPlaceholderText('Path to mpv executable');
  expect(mpvPath).toBeVisible();
  expect(mpvPath).toBeEnabled();

  const disconnect = screen.getByRole('button', { name: 'Disconnect' });
  expect(disconnect).toBeVisible();
  expect(disconnect).toBeEnabled();

  const signOut = screen.getByRole('button', { name: 'Sign out' });
  expect(signOut).toBeVisible();
  expect(signOut).toBeEnabled();

  cleanup();
});

test('library image disk cache toggle persists in config', async () => {
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const cleanup = renderConsole();

  await screen.findByDisplayValue('JellyPilot Test');
  await screen.findByRole('heading', { name: 'Library' });
  const checkbox = screen.getByRole('checkbox', { name: 'Image disk cache' });
  expect(checkbox).toBeChecked();

  fireEvent.click(checkbox);

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));
  expect(configSet).toHaveBeenCalledWith(expect.objectContaining({ imageDiskCacheEnabled: false }));

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
  provider: 'jellyfin' as const,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userId: 'user-1',
  userName: 'Ada',
};

test('session save and load round-trips through Effect', () => {
  Effect.runSync(saveSession(sampleSession));
  expect(Effect.runSync(loadSavedSession())).toEqual(sampleSession);
});

test('clearSavedSession removes the stored session', () => {
  Effect.runSync(saveSession(sampleSession));
  Effect.runSync(clearSavedSession());
  expect(Exit.isFailure(Effect.runSyncExit(loadSavedSession()))).toBe(true);
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

test('loadSavedSession fails when no session is stored', () => {
  expect(Exit.isFailure(Effect.runSyncExit(loadSavedSession()))).toBe(true);
});
