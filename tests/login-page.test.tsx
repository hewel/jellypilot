// @rstest-environment jsdom
import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { Cause, Effect, Exit } from 'effect';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import LoginPage from '../src/components/LoginPage';
import { StorageParseError } from '../src/effects/errors';
import {
  CREDENTIALS_STORAGE_KEY,
  LEGACY_CREDENTIALS_STORAGE_KEY,
  loadSavedCredentials,
} from '../src/effects/session';
import { TestQueryProvider } from './query-client';

const sampleProfiles = {
  activeProfileKey: 'jellyfin|https://jellyfin.example.com|Ada',
  profiles: [
    {
      active: true,
      key: 'jellyfin|https://jellyfin.example.com|Ada',
      lastRestoreError: null,
      reauthRequired: false,
      provider: 'jellyfin' as const,
      serverName: 'Jellyfin Home',
      serverUrl: 'https://jellyfin.example.com',
      userName: 'Ada',
    },
  ],
};
function renderLoginPage(onConnected = () => {}) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <LoginPage onConnected={onConnected} />
      </TestQueryProvider>
    ),
    root,
  );
  return () => {
    dispose();
    root.remove();
  };
}
async function fillPasswordLogin() {
  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));

  await waitFor(() => expect(screen.getByText('Username')).toBeVisible());
  fireEvent.input(screen.getByPlaceholderText('Jellyfin username'), {
    target: { value: 'ada' },
  });
  fireEvent.input(screen.getByPlaceholderText('Jellyfin password'), {
    target: { value: 'secret' },
  });
}

afterEach(() => {
  rstest.restoreAllMocks();
  rstest.useRealTimers();
  localStorage.clear();
  document.body.innerHTML = '';
});

test('login page shows quick connect as the default login method', () => {
  const cleanup = renderLoginPage();

  expect(screen.getByRole('button', { name: 'Request Quick Connect code' })).toBeVisible();
  expect(screen.getByRole('tab', { name: 'Quick Connect' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  expect(screen.queryByText('Username')).not.toBeInTheDocument();

  cleanup();
});

test('login page builds local http server url preview with jellyfin port', () => {
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: '192.168.1.20' },
  });

  expect(screen.getByText('http://192.168.1.20:8096')).toBeVisible();
  expect(screen.getByRole('button', { name: 'HTTP' })).toHaveAttribute('aria-pressed', 'true');

  cleanup();
});

test('login page rejects invalid server hosts before starting quick connect', async () => {
  const startQuickConnect = rstest.spyOn(commands, 'jellyfinQuickConnectStart');
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'not a valid host?!' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('Enter a valid Jellyfin server host')).toBeVisible());
  expect(startQuickConnect).not.toHaveBeenCalled();

  cleanup();
});

test('login page locks quick connect request while waiting for approval', async () => {
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'waiting',
    status: 'ok',
  });
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  expect(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
  ).toBeDisabled();
  expect(screen.getByRole('tab', { name: 'Password' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel Request' })).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Cancel Request' }));

  await waitFor(() =>
    expect(
      screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    ).not.toBeDisabled(),
  );

  cleanup();
});

test('login page shows password login after method selection', async () => {
  const cleanup = renderLoginPage();

  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));

  await waitFor(() => expect(screen.getByText('Username')).toBeVisible());
  expect(screen.getByPlaceholderText('Jellyfin password')).toBeVisible();
  expect(screen.getByText('Remember Server URL and username')).toBeVisible();
  expect(
    screen.queryByRole('button', { name: 'Request Quick Connect code' }),
  ).not.toBeInTheDocument();

  cleanup();
});

test('login page shows only password login for Emby', async () => {
  const cleanup = renderLoginPage();

  fireEvent.click(screen.getByRole('button', { name: 'Emby' }));

  await waitFor(() => expect(screen.getByText('Username')).toBeVisible());
  expect(screen.queryByRole('tab', { name: 'Quick Connect' })).not.toBeInTheDocument();
  expect(
    screen.queryByRole('button', { name: 'Request Quick Connect code' }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Connect' })).toBeVisible();

  cleanup();
});

test('login page completes quick connect when approval is observed', async () => {
  rstest.useFakeTimers();
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'approved',
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const saveProfile = rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  expect(saveProfile).toHaveBeenCalledTimes(1);

  cleanup();
});
test('quick connect start status errors show failure and unlock request', async () => {
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    error: { code: 'network', message: 'Server unavailable' },
    status: 'error',
  });
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('Server unavailable')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Request a new code' })).not.toBeDisabled();

  cleanup();
});

test('quick connect start rejected commands show failure and unlock request', async () => {
  rstest
    .spyOn(commands, 'jellyfinQuickConnectStart')
    .mockRejectedValue(new Error('IPC unavailable'));
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('IPC unavailable')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Request a new code' })).not.toBeDisabled();

  cleanup();
});
test('quick connect ignores a start result after switching login methods', async () => {
  let resolveStart: (
    result: Awaited<ReturnType<typeof commands.jellyfinQuickConnectStart>>,
  ) => void = () => {};
  const startResult = new Promise<Awaited<ReturnType<typeof commands.jellyfinQuickConnectStart>>>(
    (resolve) => {
      resolveStart = resolve;
    },
  );
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockReturnValue(startResult);
  const check = rstest.spyOn(commands, 'jellyfinQuickConnectCheck');
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));
  await waitFor(() => expect(screen.getByRole('button', { name: /Requesting/ })).toBeDisabled());

  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));
  await waitFor(() => expect(screen.getByText('Username')).toBeVisible());

  resolveStart({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  await Promise.resolve();

  expect(screen.queryByText('ABCD12')).not.toBeInTheDocument();
  expect(check).not.toHaveBeenCalled();

  cleanup();
});

test('quick connect polling status errors fail without changing cancel behavior', async () => {
  rstest.useFakeTimers();
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    error: { code: 'network', message: 'Approval polling failed' },
    status: 'error',
  });
  const cleanup = renderLoginPage();

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Cancel Request' })).toBeVisible();

  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(screen.getByText('Approval polling failed')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Request a new code' })).not.toBeDisabled();

  cleanup();
});

test('quick connect ignores an approval result after cancellation', async () => {
  rstest.useFakeTimers();
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  let resolveCheck: (
    result: Awaited<ReturnType<typeof commands.jellyfinQuickConnectCheck>>,
  ) => void = () => {};
  const checkResult = new Promise<Awaited<ReturnType<typeof commands.jellyfinQuickConnectCheck>>>(
    (resolve) => {
      resolveCheck = resolve;
    },
  );
  const check = rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockReturnValue(checkResult);
  const authenticate = rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate');
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);
  await waitFor(() => expect(check).toHaveBeenCalledTimes(1));

  fireEvent.click(screen.getByRole('button', { name: 'Cancel Request' }));
  expect(screen.getByRole('button', { name: 'Request Quick Connect code' })).toBeVisible();

  resolveCheck({ data: 'approved', status: 'ok' });
  await rstest.advanceTimersByTimeAsync(0);

  expect(authenticate).not.toHaveBeenCalled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});
test('quick connect can request a new code after timeout with a poll in flight', async () => {
  rstest.useFakeTimers();
  rstest
    .spyOn(commands, 'jellyfinQuickConnectStart')
    .mockResolvedValueOnce({
      data: { code: 'ABCD12', secret: 'secret-123' },
      status: 'ok',
    })
    .mockResolvedValueOnce({
      data: { code: 'WXYZ99', secret: 'secret-456' },
      status: 'ok',
    });
  const pendingCheck = new Promise<Awaited<ReturnType<typeof commands.jellyfinQuickConnectCheck>>>(
    () => {},
  );
  rstest
    .spyOn(commands, 'jellyfinQuickConnectCheck')
    .mockReturnValueOnce(pendingCheck)
    .mockResolvedValueOnce({ data: 'approved', status: 'ok' });
  rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);
  await rstest.advanceTimersByTimeAsync(5 * 60 * 1000 - 5000);

  await waitFor(() =>
    expect(
      screen.getByText('Quick Connect code expired. Request a new code to try again.'),
    ).toBeVisible(),
  );

  fireEvent.click(screen.getByRole('button', { name: 'Request a new code' }));

  await waitFor(() => expect(screen.getByText('WXYZ99')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));

  cleanup();
});

test('quick connect authentication status errors fail and unlock request', async () => {
  rstest.useFakeTimers();
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    data: { code: 'ABCD12', secret: 'secret-123' },
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    data: 'approved',
    status: 'ok',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate').mockResolvedValue({
    error: { code: 'authFailed', message: 'Authentication failed' },
    status: 'error',
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(screen.getByText('Authentication failed')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Request a new code' })).not.toBeDisabled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});

test('password login saves the authenticated session', async () => {
  const connect = rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  const saveProfile = rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'), {
    target: { value: 'jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));

  await waitFor(() => expect(screen.getByText('Username')).toBeVisible());
  fireEvent.input(screen.getByPlaceholderText('Jellyfin username'), {
    target: { value: 'ada' },
  });
  fireEvent.input(screen.getByPlaceholderText('Jellyfin password'), {
    target: { value: 'secret' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  expect(connect).toHaveBeenCalledWith({
    password: 'secret',
    provider: 'jellyfin',
    serverUrl: 'https://jellyfin.example.com',
    username: 'ada',
  });
  expect(saveProfile).toHaveBeenCalledTimes(1);

  cleanup();
});

test('password login submits the selected Emby provider', async () => {
  const connect = rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const cleanup = renderLoginPage();

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Emby' }));
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(connect).toHaveBeenCalledWith({
      password: 'secret',
      provider: 'emby',
      serverUrl: 'https://jellyfin.example.com',
      username: 'ada',
    }),
  );

  cleanup();
});
test('password login stays locked while saving the authenticated session', async () => {
  const connect = rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  let resolveSave: (profiles: typeof sampleProfiles) => void = () => {};
  const save = new Promise<typeof sampleProfiles>((resolve) => {
    resolveSave = resolve;
  });
  rstest
    .spyOn(commands, 'serverProfilesSaveCurrent')
    .mockReturnValue(save.then((profiles) => ({ data: profiles, status: 'ok' })));
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() => expect(connect).toHaveBeenCalledTimes(1));
  expect(screen.getByRole('button', { name: /Connecting/ })).toBeDisabled();

  resolveSave(sampleProfiles);
  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));

  cleanup();
});
test('password login session-save failures show an error and unlock submit', async () => {
  rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest
    .spyOn(commands, 'serverProfilesSaveCurrent')
    .mockRejectedValue(new Error('Session unavailable'));
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() => expect(screen.getByText('Session unavailable')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Connect' })).not.toBeDisabled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});
test('password login saves remembered Login Prefill when remember me is checked', async () => {
  rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const cleanup = renderLoginPage();

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('checkbox', { name: 'Remember Server URL and username' }));
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(Effect.runSync(loadSavedCredentials)).toEqual({
      rememberMe: true,
      provider: 'jellyfin',
      serverUrl: 'https://jellyfin.example.com',
      username: 'ada',
    }),
  );

  cleanup();
});

test('password login restores remembered Emby provider for Login Prefill', async () => {
  localStorage.setItem(
    CREDENTIALS_STORAGE_KEY,
    JSON.stringify({
      provider: 'emby',
      rememberMe: true,
      serverUrl: 'https://emby.example.com',
      username: 'ada',
    }),
  );
  const connect = rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const cleanup = renderLoginPage();

  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));
  await waitFor(() => expect(screen.getByDisplayValue('ada')).toBeVisible());
  fireEvent.input(screen.getByPlaceholderText('Jellyfin password'), {
    target: { value: 'secret' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(connect).toHaveBeenCalledWith({
      password: 'secret',
      provider: 'emby',
      serverUrl: 'https://emby.example.com',
      username: 'ada',
    }),
  );

  cleanup();
});

test('password login clears Login Prefill when remember me is unchecked', async () => {
  localStorage.setItem(
    CREDENTIALS_STORAGE_KEY,
    JSON.stringify({
      rememberMe: true,
      provider: 'jellyfin',
      serverUrl: 'https://old.example.com',
      username: 'old',
    }),
  );
  rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    data: null,
    status: 'ok',
  });
  rstest.spyOn(commands, 'serverProfilesSaveCurrent').mockResolvedValue({
    data: sampleProfiles,
    status: 'ok',
  });
  const cleanup = renderLoginPage();

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('checkbox', { name: 'Remember Server URL and username' }));
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() => expect(Exit.isFailure(Effect.runSyncExit(loadSavedCredentials))).toBe(true));

  cleanup();
});

test('password login status errors show the command message and unlock submit', async () => {
  rstest.spyOn(commands, 'serverConnect').mockResolvedValue({
    error: { code: 'authFailed', message: 'Invalid username or password' },
    status: 'error',
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() => expect(screen.getByText('Invalid username or password')).toBeVisible());
  expect(screen.getByRole('button', { name: 'Connect' })).not.toBeDisabled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});

test('loadSavedCredentials returns StorageParseError for malformed or wrong-shape saved inputs', () => {
  const invalidInputs = ['not json', '', JSON.stringify({ notServerUrl: true })];
  for (const input of invalidInputs) {
    localStorage.setItem(CREDENTIALS_STORAGE_KEY, input);
    const exit = Effect.runSyncExit(loadSavedCredentials);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const reason = exit.cause.reasons[0];
      if (!reason || !Cause.isFailReason(reason)) {
        throw new Error('Expected typed StorageParseError failure');
      }
      const { error } = reason;
      expect(error).toBeInstanceOf(StorageParseError);
    }
  }
});

test('loadSavedCredentials fails when no credentials are stored', () => {
  expect(Exit.isFailure(Effect.runSyncExit(loadSavedCredentials))).toBe(true);
});

test('loadSavedCredentials migrates legacy remembered Login Prefill', () => {
  localStorage.setItem(
    LEGACY_CREDENTIALS_STORAGE_KEY,
    JSON.stringify({
      rememberMe: true,
      serverUrl: 'https://old.example.com',
      username: 'old',
    }),
  );

  expect(Effect.runSync(loadSavedCredentials)).toEqual({
    rememberMe: true,
    provider: 'jellyfin',
    serverUrl: 'https://old.example.com',
    username: 'old',
  });
  expect(localStorage.getItem(LEGACY_CREDENTIALS_STORAGE_KEY)).toBeNull();
  expect(localStorage.getItem(CREDENTIALS_STORAGE_KEY)).not.toBeNull();
});

const reauthJellyfinProfile = {
  active: false,
  key: 'jellyfin|https://jellyfin.example.com|Ada',
  lastRestoreError: 'expired',
  provider: 'jellyfin' as const,
  reauthRequired: true,
  serverName: 'Jellyfin Home',
  serverUrl: 'https://jellyfin.example.com',
  userName: 'Ada',
};

function renderReauthLoginPage(onConnected = () => {}) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <LoginPage
          embedded
          onConnected={onConnected}
          reauthenticateProfile={reauthJellyfinProfile}
        />
      </TestQueryProvider>
    ),
    root,
  );
  return () => {
    dispose();
    root.remove();
  };
}

test('reauthentication mode locks identity to the saved profile', () => {
  const cleanup = renderReauthLoginPage();

  expect(screen.getByRole('heading', { name: 'Sign in again' })).toBeVisible();
  expect(
    screen.getByText('Your saved session expired. Sign in again to switch to this service.'),
  ).toBeVisible();
  expect(screen.getByText('Jellyfin')).toBeVisible();
  expect(screen.getByText('Jellyfin Home')).toBeVisible();
  expect(screen.getByText('Ada')).toBeVisible();

  expect(
    screen.queryByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
  ).not.toBeInTheDocument();
  expect(screen.queryByText('Media Server')).not.toBeInTheDocument();
  expect(screen.queryByText('Remember Server URL and username')).not.toBeInTheDocument();

  cleanup();
});

test('jellyfin recovery quick connect uses profile-scoped commands and saves no new profile', async () => {
  rstest.useFakeTimers();
  const start = rstest
    .spyOn(commands, 'serverProfilesReauthenticateQuickConnectStart')
    .mockResolvedValue({
      data: { code: 'ABCD12', secret: 'secret-123' },
      status: 'ok',
    });
  const check = rstest
    .spyOn(commands, 'serverProfilesReauthenticateQuickConnectCheck')
    .mockResolvedValue({
      data: 'approved',
      status: 'ok',
    });
  const authenticate = rstest
    .spyOn(commands, 'serverProfilesReauthenticateQuickConnectAuthenticate')
    .mockResolvedValue({
      data: sampleProfiles,
      status: 'ok',
    });
  const saveProfile = rstest.spyOn(commands, 'serverProfilesSaveCurrent');
  const standaloneStart = rstest.spyOn(commands, 'jellyfinQuickConnectStart');
  const onConnected = rstest.fn();
  const cleanup = renderReauthLoginPage(onConnected);

  fireEvent.click(screen.getByRole('button', { name: 'Request Quick Connect code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  expect(start).toHaveBeenCalledWith(reauthJellyfinProfile.key);

  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  expect(check).toHaveBeenCalledWith(reauthJellyfinProfile.key, 'secret-123');
  expect(authenticate).toHaveBeenCalledWith(reauthJellyfinProfile.key, 'secret-123');
  expect(saveProfile).not.toHaveBeenCalled();
  expect(standaloneStart).not.toHaveBeenCalled();

  cleanup();
});

test('reauthentication password sign in sends only the profile key and password', async () => {
  const reauthenticate = rstest
    .spyOn(commands, 'serverProfilesReauthenticatePassword')
    .mockResolvedValue({
      data: sampleProfiles,
      status: 'ok',
    });
  const connect = rstest.spyOn(commands, 'serverConnect');
  const saveProfile = rstest.spyOn(commands, 'serverProfilesSaveCurrent');
  const onConnected = rstest.fn();
  const cleanup = renderReauthLoginPage(onConnected);

  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));
  fireEvent.input(await screen.findByPlaceholderText('Jellyfin password'), {
    target: { value: 'fixture-password' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Sign in and switch' }));

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  expect(reauthenticate).toHaveBeenCalledWith(reauthJellyfinProfile.key, 'fixture-password');
  expect(connect).not.toHaveBeenCalled();
  expect(saveProfile).not.toHaveBeenCalled();

  cleanup();
});
