// @rstest-environment jsdom
import { afterEach, expect, test } from '@rstest/core';
import { screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import Button from '../src/components/ui/Button';

test('Button renders accessible button content and forwards button props', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <Button type="submit" disabled>
        Launch
      </Button>
    ),
    root,
  );

  const button = screen.getByRole('button', { name: 'Launch' });
  expect(button).toHaveAttribute('type', 'submit');
  expect(button).toBeDisabled();

  dispose();
  root.remove();
});

test('Button renders as a link when href is provided', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <Button href="/library">Open Library</Button>, root);

  expect(screen.getByRole('link', { name: 'Open Library' })).toHaveAttribute('href', '/library');

  dispose();
  root.remove();
});

test('Button icon variant preserves accessible button behavior', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <Button variant="icon" aria-label="Close panel">
        <span aria-hidden="true">x</span>
      </Button>
    ),
    root,
  );

  expect(screen.getByRole('button', { name: 'Close panel' })).toBeEnabled();

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
});
