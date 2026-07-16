import { afterEach, expect, rstest, test } from '@rstest/core';
import * as tauriApp from '@tauri-apps/api/app';
import { screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import AppVersion from '../src/components/AppVersion';
import * as styles from '../src/components/AppVersion.styles';
import { TestQueryProvider } from './query-client';

test('AppVersion renders the package version with the Panda canary class', async () => {
  rstest.spyOn(tauriApp, 'getVersion').mockResolvedValue('1.4.1');
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <AppVersion />
      </TestQueryProvider>
    ),
    root,
  );

  const label = await screen.findByText('v1.4.1');
  expect(label.tagName).toBe('P');
  expect(label.className).toContain(styles.version);

  dispose();
  root.remove();
});

test('AppVersion accepts an external class override without changing the API', async () => {
  rstest.spyOn(tauriApp, 'getVersion').mockResolvedValue('9.9.9');
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <TestQueryProvider>
        <AppVersion class="external-class" />
      </TestQueryProvider>
    ),
    root,
  );

  const label = await screen.findByText('v9.9.9');
  expect(label.className).toBe('external-class');

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
  rstest.restoreAllMocks();
});
