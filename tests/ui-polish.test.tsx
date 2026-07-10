import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { ToastProvider, useToast } from '../src/components/ToastProvider';

function ToastTrigger() {
  const { showToast } = useToast();

  return (
    <button type="button" onClick={() => showToast('error', 'Save failed')}>
      Show toast
    </button>
  );
}

test('app toast adapter renders and dismisses UI Core feedback', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <ToastProvider>
        <ToastTrigger />
      </ToastProvider>
    ),
    root,
  );

  fireEvent.click(screen.getByRole('button', { name: 'Show toast' }));

  const toast = screen.getByRole('status');
  expect(toast).toHaveTextContent('Save failed');
  expect(toast.closest('[data-ui="toast"]')).not.toBeNull();
  expect(toast).toHaveAttribute('data-tone', 'danger');

  fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
  expect(screen.queryByRole('status')).toBeNull();

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
});
