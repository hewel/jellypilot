import { afterEach, expect, test } from '@rstest/core';
import { render } from 'solid-js/web';

import Toast from '../src/components/Toast';
import Button from '../src/components/ui/Button';

test('Button icon sm exposes a 40px hit area and 0.96 press scale', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <Button variant="icon" size="sm">
        x
      </Button>
    ),
    root,
  );

  const button = root.querySelector('button')!;
  expect(button.className).toContain('h-10');
  expect(button.className).toContain('w-10');
  expect(button.className).toContain('active:scale-[0.96]');

  dispose();
  root.remove();
});

test('Toast exit state applies slide-out classes and a 40px close target', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => <Toast id="toast-1" level="info" message="Saved" exiting onDismiss={() => undefined} />,
    root,
  );

  const alert = root.querySelector('[role="alert"]')!;
  expect(alert.className).toContain('translate-y-1');
  expect(alert.className).toContain('opacity-0');
  expect(alert.className).toContain('blur-[2px]');

  const close = root.querySelector('button[aria-label="Close"]')!;
  expect(close.className).toContain('h-10');
  expect(close.className).toContain('w-10');

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
});
