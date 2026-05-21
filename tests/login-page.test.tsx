import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import { commands } from '../src/bindings';
import LoginPage from '../src/components/LoginPage';

afterEach(() => {
  rstest.restoreAllMocks();
  rstest.useRealTimers();
  localStorage.clear();
  document.body.innerHTML = '';
});

function renderLoginPage(onConnected = () => undefined) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <LoginPage onConnected={onConnected} />, root);
  return () => {
    dispose();
    root.remove();
  };
}

test('login page shows quick connect as the default login method', () => {
  const cleanup = renderLoginPage();

  expect(screen.getByRole('button', { name: 'Request code' })).toBeVisible();
  expect(
    screen.getByRole('button', { name: 'Use password instead' }),
  ).toBeVisible();
  expect(screen.queryByLabelText('Username')).not.toBeInTheDocument();

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

  fireEvent.input(screen.getByLabelText('Server URL'), {
    target: { value: 'https://jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  expect(screen.getByLabelText('Server URL')).toBeDisabled();
  expect(
    screen.getByRole('button', { name: 'Use password instead' }),
  ).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

  await waitFor(() =>
    expect(screen.getByLabelText('Server URL')).not.toBeDisabled(),
  );
  expect(screen.getByRole('button', { name: 'Request code' })).toBeVisible();

  cleanup();
});

test('login page shows password login after fallback toggle', async () => {
  const cleanup = renderLoginPage();

  fireEvent.click(screen.getByRole('button', { name: 'Use password instead' }));

  await waitFor(() => expect(screen.getByLabelText('Username')).toBeVisible());
  expect(screen.getByLabelText('Password')).toBeVisible();
  expect(screen.getByLabelText('Remember server and username')).toBeVisible();
  expect(
    screen.getByRole('button', { name: 'Use Quick Connect' }),
  ).toBeVisible();
  expect(
    screen.queryByRole('button', { name: 'Request code' }),
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
  rstest.spyOn(commands, 'jellyfinGetSession').mockResolvedValue({
    serverUrl: 'https://jellyfin.example.com',
    accessToken: 'token-1',
    userId: 'user-1',
    userName: 'Ada',
    serverName: 'Jellyfin Home',
    deviceId: 'device-1',
  });
  const onConnected = rstest.fn();
  const cleanup = renderLoginPage(onConnected);

  fireEvent.input(screen.getByLabelText('Server URL'), {
    target: { value: 'https://jellyfin.example.com' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Request code' }));

  await waitFor(() => expect(screen.getByText('ABCD12')).toBeVisible());
  await rstest.advanceTimersByTimeAsync(5000);

  await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));

  cleanup();
});
