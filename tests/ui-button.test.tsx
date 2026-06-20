import { afterEach, expect, test } from '@rstest/core';
import { render } from 'solid-js/web';

import Button from '../src/components/ui/Button';

test('Button primary renders Tailwind gradient atoms without leaking vanilla-extract class names', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <Button variant="primary">Launch</Button>, root);

  const button = root.querySelector('button')!;
  expect(button.className).not.toMatch(/baseButton|variantStyles_|sizeStyles_|iconSizeStyles_/);
  expect(button.className).toContain('from-primary');
  expect(button.className).toContain('to-primary-gradient-end');

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
});
