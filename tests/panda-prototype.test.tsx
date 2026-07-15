import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { PandaPrototypePage } from '../src/routes/panda-prototype';

Element.prototype.scrollTo = () => {};

afterEach(() => {
  document.body.innerHTML = '';
});

test('Panda prototype preserves Ark selection and Solid state behavior', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <PandaPrototypePage />, root);

  const standardSelect = screen.getByRole('combobox', { name: 'Standard audio track' });
  expect(standardSelect.closest('[data-scope="select"]')).not.toBeNull();
  expect(standardSelect).toHaveTextContent('English — Stereo');

  fireEvent.click(standardSelect);
  fireEvent.click(await screen.findByRole('option', { name: 'Japanese — 5.1' }));
  await waitFor(() => expect(standardSelect).toHaveTextContent('Japanese — 5.1'));

  const toggle = screen.getByRole('button', { name: 'Toggle' });
  expect(toggle).toHaveAttribute('aria-pressed', 'true');
  expect(screen.getByText('Connected')).toBeVisible();

  fireEvent.click(toggle);
  expect(toggle).toHaveAttribute('aria-pressed', 'false');
  expect(screen.getByText('Disconnected')).toBeVisible();

  dispose();
  root.remove();
});
