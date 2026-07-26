import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen } from '@testing-library/dom';
import { render } from 'solid-js/web';

import Toast from '../src/components/Toast';
import Button from '../src/components/ui/Button';
import { indicatorStyles } from '../src/components/ui/SegmentedControl.styles';
import StatusBadge from '../src/components/ui/StatusBadge';
import { reducedMotionFeedback } from '../src/styles/recipes';

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

test('StatusBadge accepts info and renders visible text plus its indicator', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <StatusBadge variant="info">Connected</StatusBadge>, root);

  const badge = screen.getByText('Connected');
  expect(badge).toBeVisible();
  // The LED dot renders alongside the text; color is never the only signal.
  expect(badge.querySelector('span')).not.toBeNull();

  dispose();
  root.remove();
});

test('reduced motion disables spatial feedback and caps remaining transitions at 100ms', () => {
  expect(reducedMotionFeedback.transform).toBe('[none]');
  expect(reducedMotionFeedback.transitionDuration).toBe('100');

  // The segmented indicator stops sliding; only color feedback may remain, capped at 100ms.
  expect(indicatorStyles._motionReduce).toEqual({
    transitionDuration: '100',
    transitionProperty: '[background-color, border-color, box-shadow]',
  });
  expect(String(indicatorStyles.transitionProperty)).toContain('top');
});

afterEach(() => {
  document.body.innerHTML = '';
});
