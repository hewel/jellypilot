import { expect, test } from '@rstest/core';
import { screen, within } from '@testing-library/dom';
import { render } from 'solid-js/web';

import { GenrePills, LibraryStatusPanel } from '../src/components/library/shared';

test('GenrePills renders nothing for an empty genre list', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <GenrePills genres={[]} />, root);

  expect(root.querySelector('[role="list"]')).toBeNull();

  dispose();
  root.remove();
});

test('GenrePills renders each genre as a listitem inside a list container', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => <GenrePills genres={['Drama', 'Sci-Fi']} />, root);

  const list = screen.getByRole('list');
  const items = within(list).getAllByRole('listitem');
  expect(items.map((item) => item.textContent)).toEqual(['Drama', 'Sci-Fi']);

  dispose();
  root.remove();
});

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
