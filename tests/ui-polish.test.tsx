import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import Toast from '../src/components/Toast';
import Button from '../src/components/ui/Button';

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

test('Toast exposes alert content and dismisses from the close button', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dismissed: string[] = [];
  const dispose = render(
    () => (
      <Toast
        id="toast-1"
        level="info"
        message="Saved"
        exiting={false}
        onDismiss={(id) => dismissed.push(id)}
      />
    ),
    root,
  );

  expect(screen.getByRole('alert')).toHaveTextContent('Saved');

  fireEvent.click(screen.getByRole('button', { name: 'Close' }));
  expect(dismissed).toEqual(['toast-1']);

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
});
