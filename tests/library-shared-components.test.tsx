// @rstest-environment jsdom
import { expect, test } from '@rstest/core';
import { render } from 'solid-js/web';

import { LibraryStatusPanel } from '../src/components/library/shared';

test('LibraryStatusPanel instances expose distinct labelled-by ids', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <>
        <LibraryStatusPanel title="First panel" />
        <LibraryStatusPanel title="Second panel" />
      </>
    ),
    root,
  );

  const sections = root.querySelectorAll('section[aria-labelledby]');
  expect(sections.length).toBe(2);

  const first = sections[0];
  const second = sections[1];
  expect(first).toBeDefined();
  expect(second).toBeDefined();
  const firstLabelId = first.getAttribute('aria-labelledby');
  const secondLabelId = second.getAttribute('aria-labelledby');
  expect(firstLabelId).not.toBeNull();
  expect(secondLabelId).not.toBeNull();
  expect(firstLabelId).not.toBe(secondLabelId);

  expect(first.querySelector(`#${CSS.escape(firstLabelId!)}`)?.textContent).toBe('First panel');
  expect(second.querySelector(`#${CSS.escape(secondLabelId!)}`)?.textContent).toBe('Second panel');

  dispose();
  root.remove();
});
