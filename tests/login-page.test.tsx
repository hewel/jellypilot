import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import { commands } from '../src/bindings';
import LoginPage from '../src/components/LoginPage';

function renderLoginPage(onConnected = () => undefined) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <LoginPage onConnected={onConnected} />, root);
  return () => {
    dispose();
    root.remove();
  };
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

  cleanup();
});
