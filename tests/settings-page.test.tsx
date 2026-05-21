import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';
import { commands } from '../src/bindings';
import SettingsPage from '../src/components/SettingsPage';
import { ToastProvider } from '../src/components/ToastProvider';

afterEach(() => {
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});

function renderSettingsPage() {
  rstest.spyOn(commands, 'jellyfinGetState').mockResolvedValue({
    connected: true,
    serverUrl: 'https://jellyfin.example.com',
    serverName: 'Jellyfin Home',
    userName: 'Ada',
  });
  rstest.spyOn(commands, 'mpvIsConnected').mockResolvedValue(false);

  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <SettingsPage onDisconnected={() => undefined} />
      </ToastProvider>
    ),
    root,
  );

  return () => {
    dispose();
    root.remove();
  };
}

test('settings page loads intro skipper setting from config', async () => {
  rstest.spyOn(commands, 'configGet').mockResolvedValue({
    deviceName: 'JMSR Test',
    mpvPath: null,
    mpvArgs: [],
    progressInterval: 5,
    startMinimized: false,
    introSkipperEnabled: false,
    keybindNext: 'Shift+n',
    keybindPrev: 'Shift+p',
  });
  const cleanup = renderSettingsPage();

  await waitFor(() =>
    expect(screen.getByLabelText('Intro Skipper')).not.toBeChecked(),
  );

  cleanup();
});

test('settings page saves changed intro skipper setting', async () => {
  rstest.spyOn(commands, 'configGet').mockResolvedValue({
    deviceName: 'JMSR Test',
    mpvPath: null,
    mpvArgs: [],
    progressInterval: 5,
    startMinimized: false,
    introSkipperEnabled: true,
    keybindNext: 'Shift+n',
    keybindPrev: 'Shift+p',
  });
  const configSet = rstest.spyOn(commands, 'configSet').mockResolvedValue({
    status: 'ok',
    data: null,
  });
  const cleanup = renderSettingsPage();

  await waitFor(() =>
    expect(screen.getByDisplayValue('JMSR Test')).toBeVisible(),
  );
  await waitFor(() =>
    expect(screen.getByLabelText('Intro Skipper')).toBeChecked(),
  );
  const checkbox = screen.getByLabelText('Intro Skipper') as HTMLInputElement;
  fireEvent.click(checkbox);
  await waitFor(() => expect(checkbox).not.toBeChecked());
  fireEvent.click(screen.getByRole('button', { name: 'Save Settings' }));

  await waitFor(() => expect(configSet).toHaveBeenCalledTimes(1));
  expect(configSet).toHaveBeenCalledWith(
    expect.objectContaining({ introSkipperEnabled: false }),
  );

  cleanup();
});

test('settings page keeps diagnostics under collapsed troubleshooting', async () => {
  rstest.spyOn(commands, 'configGet').mockResolvedValue({
    deviceName: 'JMSR Test',
    mpvPath: null,
    mpvArgs: [],
    progressInterval: 5,
    startMinimized: false,
    introSkipperEnabled: true,
    keybindNext: 'Shift+n',
    keybindPrev: 'Shift+p',
  });
  const cleanup = renderSettingsPage();

  await waitFor(() =>
    expect(screen.getByText('Troubleshooting')).toBeVisible(),
  );
  expect(screen.getByText('Diagnostics')).not.toBeVisible();

  fireEvent.click(
    screen.getByRole('button', { name: 'Toggle troubleshooting' }),
  );

  expect(screen.getByText('Diagnostics')).toBeVisible();

  cleanup();
});
