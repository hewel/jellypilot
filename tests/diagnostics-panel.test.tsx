import { afterEach, beforeEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import DiagnosticsPanel from '../src/components/DiagnosticsPanel';

type LogHandler = (event: { payload: { level: number; message: string } }) => void;

declare global {
  interface Window {
    __TAURI_INTERNALS__: {
      transformCallback: (callback: LogHandler) => number;
    };
  }
}

let logHandler: LogHandler | null = null;
let clipboardWriteText: ReturnType<typeof rstest.fn>;

beforeEach(() => {
  logHandler = null;
  window.__TAURI_INTERNALS__.transformCallback = (callback: LogHandler) => {
    logHandler = callback;
    return 1;
  };
  clipboardWriteText = rstest.fn().mockResolvedValue();
  Object.assign(navigator, {
    clipboard: {
      writeText: clipboardWriteText,
    },
  });
});

afterEach(() => {
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});

function renderDiagnosticsPanel() {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <DiagnosticsPanel />, root);

  return () => {
    dispose();
    root.remove();
  };
}

async function emitLog(level: number, message: string) {
  await waitFor(() => expect(logHandler).not.toBeNull());
  logHandler?.({ payload: { level, message } });
}

test('diagnostics panel shows backend log entries with timestamps', async () => {
  const cleanup = renderDiagnosticsPanel();

  await emitLog(3, 'WebSocket reconnected successfully');

  await waitFor(() => expect(screen.getByText('WebSocket reconnected successfully')).toBeVisible());
  expect(screen.getByText('INFO')).toBeVisible();
  expect(screen.getByText(/\d{2}:\d{2}:\d{2}/)).toBeVisible();

  cleanup();
});

test('diagnostics panel keeps the latest 200 entries', async () => {
  const cleanup = renderDiagnosticsPanel();

  for (let i = 0; i < 201; i += 1) {
    await emitLog(3, `event ${i}`);
  }

  await waitFor(() => expect(screen.getByText('200 sanitized runtime events')).toBeVisible());
  expect(screen.queryByText('event 0')).toBeNull();
  expect(screen.getByText('event 200')).toBeVisible();

  cleanup();
});

test('diagnostics panel copies visible diagnostics', async () => {
  const cleanup = renderDiagnosticsPanel();

  await emitLog(4, 'MPV IPC connection closed');
  fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostics' }));

  await waitFor(() => expect(clipboardWriteText).toHaveBeenCalledTimes(1));
  expect(clipboardWriteText).toHaveBeenCalledWith(
    expect.stringContaining('WARN MPV IPC connection closed'),
  );
  expect(screen.getByText('Copied')).toBeVisible();

  cleanup();
});

test('diagnostics panel redacts secret-bearing values', async () => {
  const cleanup = renderDiagnosticsPanel();

  await emitLog(
    5,
    'Loading file: https://jellyfin.example.com/Videos/1/stream?api_key=secret-token&MediaSourceId=source-1',
  );
  fireEvent.click(screen.getByRole('button', { name: 'Copy diagnostics' }));

  await waitFor(() => expect(screen.getByText(/api_key=\[REDACTED\]/)).toBeVisible());
  expect(screen.queryByText(/secret-token/)).toBeNull();
  expect(clipboardWriteText).toHaveBeenCalledWith(expect.stringContaining('api_key=[REDACTED]'));
  expect(clipboardWriteText).not.toHaveBeenCalledWith(expect.stringContaining('secret-token'));

  cleanup();
});
test('diagnostics auto-scroll uses the public checkbox contract', () => {
  const cleanup = renderDiagnosticsPanel();

  const checkbox = screen.getByRole('checkbox', { name: 'Auto-scroll' });
  expect(checkbox).toBeChecked();
  expect(checkbox.closest('[data-ui="checkbox-input"]')).not.toBeNull();
  expect(document.querySelector('[data-ui="checkbox-input"] [data-part="control"]')).not.toBeNull();

  fireEvent.click(checkbox);
  expect(checkbox).not.toBeChecked();

  cleanup();
});
