// @rstest-environment jsdom
import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import LibrarySettingsCard from '../src/components/OperationsConsole/LibrarySettingsCard';
import { ToastProvider } from '../src/components/ToastProvider';
import { TestQueryProvider } from './query-client';

const populatedStatus = {
  committedBytes: 5 * 1024 * 1024,
  pendingCount: 2,
  estimatedSavings: 1024 * 1024,
  terminalFailures: 1,
  enabled: true,
  clearing: false,
};

function mockStatus(data = populatedStatus) {
  rstest.spyOn(commands, 'imageCacheStatus').mockResolvedValue({ status: 'ok', data });
}

function renderCard(enabled: boolean) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <ToastProvider>
          <LibrarySettingsCard
            imageDiskCacheEnabled={enabled}
            onImageDiskCacheEnabledChange={() => {}}
          />
        </ToastProvider>
      </TestQueryProvider>
    ),
    root,
  );
  return { dispose, root };
}

afterEach(() => {
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});

test('disabled state explains bypass while cached artwork is retained', () => {
  mockStatus({ ...populatedStatus, enabled: false });
  const { dispose } = renderCard(false);

  expect(screen.getByTestId('image-cache-disabled-copy').textContent).toMatch(/bypassed.*kept/i);
  dispose();
});

test('shows current cache usage, savings, pending, and failures from status', async () => {
  mockStatus();
  const { dispose } = renderCard(true);

  await waitFor(() => expect(screen.getByTestId('image-cache-usage')).toHaveTextContent('5.0 MB'));
  expect(screen.getByTestId('image-cache-savings')).toHaveTextContent('1.0 MB');
  expect(screen.getByTestId('image-cache-pending')).toHaveTextContent('2');
  expect(screen.getByTestId('image-cache-failures')).toHaveTextContent('1');
  dispose();
});

test('clear requires confirmation and cancel does not invoke the clear command', async () => {
  mockStatus();
  const clearCommand = rstest
    .spyOn(commands, 'imageCacheClear')
    .mockResolvedValue({ status: 'ok', data: { ...populatedStatus, committedBytes: 0 } });
  const { dispose } = renderCard(true);

  const trigger = await screen.findByRole('button', { name: 'Clear cache' });
  await waitFor(() => expect(trigger).not.toBeDisabled());
  fireEvent.click(trigger);
  expect(await screen.findByText('Clear Library image cache?')).toBeVisible();

  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await waitFor(() => expect(screen.queryByText('Clear Library image cache?')).toBeNull());
  expect(clearCommand).not.toHaveBeenCalled();
  dispose();
});

test('clear stays disabled while a clearing epoch is in progress', async () => {
  mockStatus({ ...populatedStatus, clearing: true });
  const { dispose } = renderCard(true);

  const trigger = await screen.findByRole('button', { name: 'Clear cache' });
  await waitFor(() => expect(screen.getByTestId('image-cache-usage')).toHaveTextContent('5.0 MB'));
  expect(trigger).toBeDisabled();
  dispose();
});

test('confirming clear invokes the clear command once', async () => {
  mockStatus();
  const clearCommand = rstest
    .spyOn(commands, 'imageCacheClear')
    .mockResolvedValue({ status: 'ok', data: { ...populatedStatus, committedBytes: 0 } });
  const { dispose } = renderCard(true);

  const trigger = await screen.findByRole('button', { name: 'Clear cache' });
  await waitFor(() => expect(trigger).not.toBeDisabled());
  fireEvent.click(trigger);
  fireEvent.click(await screen.findByTestId('image-cache-clear-confirm'));

  await waitFor(() => expect(clearCommand).toHaveBeenCalledTimes(1));
  dispose();
});
