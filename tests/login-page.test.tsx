import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { Cause, Effect, Exit } from 'effect';
import { render } from 'solid-js/web';
import { commands } from '../src/bindings';
import LoginPage from '../src/components/LoginPage';
import { StorageParseError } from '../src/effects/errors';
import {
  CREDENTIALS_STORAGE_KEY,
  loadSavedCredentials,
} from '../src/effects/session';
import { loadSavedSession } from '../src/sessionAccess';

const sampleSession = {
  serverUrl: 'https://jellyfin.example.com',
  accessToken: 'token-1',
  userId: 'user-1',
  userName: 'Ada',
  serverName: 'Jellyfin Home',
  deviceId: 'device-1',
};
function renderLoginPage(onConnected = () => undefined) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <LoginPage onConnected={onConnected} />, root);
  return () => {
    dispose();
    root.remove();
  };
}
async function fillPasswordLogin() {
  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'jellyfin.example.com' },
    },
  );
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

  expect(
    screen.getByRole('button', { name: 'Request Quick Connect code' }),
  ).toBeVisible();
  expect(screen.getByRole('tab', { name: 'Quick Connect' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  expect(screen.queryByText('Username')).not.toBeInTheDocument();

  cleanup();
});
test('login page uses Ark tabs, fields, and checkbox primitives', async () => {
  const cleanup = renderLoginPage();

  const quickConnectTab = screen.getByRole('tab', { name: 'Quick Connect' });
  expect(quickConnectTab.closest('[data-scope="tabs"]')).not.toBeNull();

  const serverHost = screen.getByPlaceholderText(
    'jellyfin.local or media.example.com/jellyfin',
  );
  expect(serverHost.closest('[data-scope="field"]')).not.toBeNull();

  fireEvent.click(screen.getByRole('tab', { name: 'Password' }));

  await waitFor(() =>
    expect(screen.getByText('Remember Server URL and username')).toBeVisible(),
  );
  const rememberMe = screen.getByRole('checkbox', {
    name: 'Remember Server URL and username',
  });
  expect(rememberMe.closest('[data-scope="checkbox"]')).not.toBeNull();
  fireEvent.click(rememberMe);
  expect(rememberMe).toBeChecked();

  cleanup();
});

test('login page builds local http server url preview with jellyfin port', () => {
  const cleanup = renderLoginPage();

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: '192.168.1.20' },
    },
  );

  expect(screen.getByText('http://192.168.1.20:8096')).toBeVisible();
  expect(screen.getByRole('button', { name: 'HTTP' })).toHaveClass(
    'bg-primary',
  );

  cleanup();
});

test('login page preserves explicit pasted schemes', () => {
  const cleanup = renderLoginPage();

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'http://media.example.com' },
    },
  );
  expect(screen.getByText('http://media.example.com')).toBeVisible();
  expect(screen.getByRole('button', { name: 'HTTP' })).toHaveClass(
    'bg-primary',
  );

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'https://192.168.1.20:8096' },
    },
  );
  expect(screen.getByText('https://192.168.1.20:8096')).toBeVisible();
  expect(screen.getByRole('button', { name: 'HTTPS' })).toHaveClass(
    'bg-primary',
  );

  cleanup();
});

test('login page preserves public reverse proxy path without default jellyfin port', () => {
  const cleanup = renderLoginPage();

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'media.example.com/jellyfin' },
    },
  );

  expect(screen.getByText('https://media.example.com/jellyfin')).toBeVisible();

  cleanup();
});

test('login page rejects invalid server hosts before starting quick connect', async () => {
  const startQuickConnect = rstest.spyOn(commands, 'jellyfinQuickConnectStart');
  const cleanup = renderLoginPage();

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'not a valid host?!' },
    },
  );
  fireEvent.click(
    screen.getByRole('button', { name: 'Request Quick Connect code' }),
  );

  await waitFor(() =>
    expect(
      screen.getByText('Enter a valid Jellyfin server host'),
    ).toBeVisible(),
  );
  expect(startQuickConnect).not.toHaveBeenCalled();

  cleanup();
});

test('login page locks quick connect request while waiting for approval', async () => {
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    status: 'ok',
    data: { code: 'ABCD12', secret: 'secret-123' },
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    status: 'ok',
    data: 'waiting',
  });
  const cleanup = renderLoginPage();

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'jellyfin.example.com' },
    },
  );
  fireEvent.click(
    screen.getByRole('button', { name: 'Request Quick Connect code' }),
  );

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  expect(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
  ).toBeDisabled();
  expect(screen.getByRole('tab', { name: 'Password' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel Request' })).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Cancel Request' }));

  await waitFor(() =>
    expect(
      screen.getByPlaceholderText(
        'jellyfin.local or media.example.com/jellyfin',
      ),
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

test('login page completes quick connect when approval is observed', async () => {
  rstest.useFakeTimers();
  rstest.spyOn(commands, 'jellyfinQuickConnectStart').mockResolvedValue({
    status: 'ok',
    data: { code: 'ABCD12', secret: 'secret-123' },
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectCheck').mockResolvedValue({
    status: 'ok',
    data: 'approved',
  });
  rstest.spyOn(commands, 'jellyfinQuickConnectAuthenticate').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  rstest.spyOn(commands, 'jellyfinGetSession').mockResolvedValue(sampleSession);
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'jellyfin.example.com' },
    },
  );
  fireEvent.click(
    screen.getByRole('button', { name: 'Request Quick Connect code' }),
  );

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  expect(loadSavedSession()).toEqual(sampleSession);

  cleanup();
});

test('password login saves the authenticated session', async () => {
  rstest.spyOn(commands, 'jellyfinConnect').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  rstest.spyOn(commands, 'jellyfinGetSession').mockResolvedValue(sampleSession);
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(
    screen.getByPlaceholderText('jellyfin.local or media.example.com/jellyfin'),
    {
      target: { value: 'jellyfin.example.com' },
    },
  );
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
  expect(loadSavedSession()).toEqual(sampleSession);

  cleanup();
});
test('password login stays locked while saving the authenticated session', async () => {
  const connect = rstest.spyOn(commands, 'jellyfinConnect').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  let resolveSession: (session: typeof sampleSession) => void = () => undefined;
  const session = new Promise<typeof sampleSession>((resolve) => {
    resolveSession = resolve;
  });
  rstest.spyOn(commands, 'jellyfinGetSession').mockReturnValue(session);
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() => expect(connect).toHaveBeenCalledTimes(1));
  expect(screen.getByRole('button', { name: /Connecting/ })).toBeDisabled();

  resolveSession(sampleSession);
  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));

  cleanup();
});
test('password login session-save failures show an error and unlock submit', async () => {
  rstest.spyOn(commands, 'jellyfinConnect').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  rstest
    .spyOn(commands, 'jellyfinGetSession')
    .mockRejectedValue(new Error('Session unavailable'));
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(screen.getByText('Session unavailable')).toBeVisible(),
  );
  expect(screen.getByRole('button', { name: 'Connect' })).not.toBeDisabled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});
test('password login saves remembered Login Prefill when remember me is checked', async () => {
  rstest.spyOn(commands, 'jellyfinConnect').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  rstest.spyOn(commands, 'jellyfinGetSession').mockResolvedValue(sampleSession);
  const cleanup = renderLoginPage();

  await fillPasswordLogin();
  fireEvent.click(
    screen.getByRole('checkbox', { name: 'Remember Server URL and username' }),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(Effect.runSync(loadSavedCredentials())).toEqual({
      serverUrl: 'https://jellyfin.example.com',
      username: 'ada',
      rememberMe: true,
    }),
  );

  cleanup();
});

test('password login clears Login Prefill when remember me is unchecked', async () => {
  localStorage.setItem(
    CREDENTIALS_STORAGE_KEY,
    JSON.stringify({
      serverUrl: 'https://old.example.com',
      username: 'old',
      rememberMe: true,
    }),
  );
  rstest.spyOn(commands, 'jellyfinConnect').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  rstest.spyOn(commands, 'jellyfinGetSession').mockResolvedValue(sampleSession);
  const cleanup = renderLoginPage();

  await fillPasswordLogin();
  fireEvent.click(
    screen.getByRole('checkbox', { name: 'Remember Server URL and username' }),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(Effect.runSync(loadSavedCredentials())).toBeNull(),
  );

  cleanup();
});

test('password login status errors show the command message and unlock submit', async () => {
  rstest.spyOn(commands, 'jellyfinConnect').mockResolvedValue({
    status: 'error',
    error: { code: 'authFailed', message: 'Invalid username or password' },
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(screen.getByText('Invalid username or password')).toBeVisible(),
  );
  expect(screen.getByRole('button', { name: 'Connect' })).not.toBeDisabled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});

test('password login rejected commands show an error and unlock submit', async () => {
  rstest
    .spyOn(commands, 'jellyfinConnect')
    .mockRejectedValue(new Error('IPC unavailable'));
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  await fillPasswordLogin();
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

  await waitFor(() =>
    expect(screen.getByText('IPC unavailable')).toBeVisible(),
  );
  expect(screen.getByRole('button', { name: 'Connect' })).not.toBeDisabled();
  expect(onConnected).not.toHaveBeenCalled();

  cleanup();
});

test('loadSavedCredentials returns StorageParseError for malformed JSON', () => {
  localStorage.setItem(CREDENTIALS_STORAGE_KEY, 'not json');
  const exit = Effect.runSyncExit(loadSavedCredentials());
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const reason = exit.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed StorageParseError failure');
    }
    const error = reason.error;
    expect(error).toBeInstanceOf(StorageParseError);
    expect(error.key).toBe(CREDENTIALS_STORAGE_KEY);
  }
});
test('loadSavedCredentials returns StorageParseError for empty malformed JSON', () => {
  localStorage.setItem(CREDENTIALS_STORAGE_KEY, '');
  const exit = Effect.runSyncExit(loadSavedCredentials());
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const reason = exit.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed StorageParseError failure');
    }
    const error = reason.error;
    expect(error).toBeInstanceOf(StorageParseError);
    expect(error.key).toBe(CREDENTIALS_STORAGE_KEY);
  }
});

test('loadSavedCredentials returns StorageParseError for wrong shape', () => {
  localStorage.setItem(
    CREDENTIALS_STORAGE_KEY,
    JSON.stringify({ notServerUrl: true }),
  );
  const exit = Effect.runSyncExit(loadSavedCredentials());
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const reason = exit.cause.reasons[0];
    if (!reason || !Cause.isFailReason(reason)) {
      throw new Error('Expected typed StorageParseError failure');
    }
    const error = reason.error;
    expect(error).toBeInstanceOf(StorageParseError);
  }
});

test('loadSavedCredentials returns null for missing key', () => {
  const exit = Effect.runSyncExit(loadSavedCredentials());
  expect(Exit.isSuccess(exit)).toBe(true);
  if (Exit.isSuccess(exit)) {
    expect(exit.value).toBeNull();
  }
});
