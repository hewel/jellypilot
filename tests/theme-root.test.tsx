import { afterEach, expect, rstest, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { commands } from '../src/bindings';
import { ConfigGate } from '../src/components/ConfigGate';
import { useConfigCoordinator } from '../src/effects/configContext';

function installColorScheme(initialDark: boolean) {
  let dark = initialDark;
  const listeners = new Set<() => void>();
  const original = window.matchMedia;

  window.matchMedia = ((query: string) => ({
    get matches() {
      return query === '(prefers-color-scheme: dark)' && dark;
    },
    media: query,
    onchange: null,
    addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
    addListener: (listener: () => void) => listeners.add(listener),
    removeListener: (listener: () => void) => listeners.delete(listener),
    dispatchEvent: () => true,
  })) as typeof window.matchMedia;

  return {
    restore: () => {
      window.matchMedia = original;
    },
    setDark: (next: boolean) => {
      dark = next;
      listeners.forEach((listener) => listener());
    },
  };
}

function ThemeControls() {
  const { coordinator } = useConfigCoordinator();

  return (
    <div data-testid="theme-content">
      <button type="button" onClick={() => coordinator.setThemePreference('light')}>
        Use light
      </button>
      <button type="button" onClick={() => coordinator.setThemePreference('dark')}>
        Use dark
      </button>
    </div>
  );
}

test('UIRoot resolves system preference and switches concrete theme attributes', async () => {
  const colorScheme = installColorScheme(false);
  rstest.spyOn(commands, 'configGet').mockResolvedValue({ themePreference: 'system' });
  rstest.spyOn(commands, 'configSet').mockResolvedValue({ data: null, status: 'ok' });
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ConfigGate>
        <ThemeControls />
      </ConfigGate>
    ),
    root,
  );

  try {
    await screen.findByTestId('theme-content');
    await waitFor(() => expect(document.documentElement).toHaveAttribute('data-theme', 'light'));
    expect(document.documentElement).toHaveAttribute('data-theme-id', 'jellypilot');
    expect(document.querySelector('[data-jp-uiroot]')).toHaveAttribute('data-theme', 'light');

    colorScheme.setDark(true);
    await waitFor(() => expect(document.documentElement).toHaveAttribute('data-theme', 'dark'));
    expect(document.querySelector('[data-jp-uiroot]')).toHaveAttribute('data-theme', 'dark');

    fireEvent.click(screen.getByRole('button', { name: 'Use light' }));
    await waitFor(() => expect(document.documentElement).toHaveAttribute('data-theme', 'light'));
    expect(document.querySelector('[data-jp-uiroot]')).toHaveAttribute('data-theme', 'light');

    fireEvent.click(screen.getByRole('button', { name: 'Use dark' }));
    await waitFor(() => expect(document.documentElement).toHaveAttribute('data-theme', 'dark'));
    expect(document.querySelector('[data-jp-uiroot]')).toHaveAttribute('data-theme', 'dark');
  } finally {
    dispose();
    root.remove();
    colorScheme.restore();
  }
});

afterEach(() => {
  rstest.restoreAllMocks();
  document.body.innerHTML = '';
});
