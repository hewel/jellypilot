import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

import { SegmentedControl } from '../src/components/ui';
import { segmentedControlNarrowLayout } from '../src/components/ui/SegmentedControl.styles';

afterEach(() => {
  document.body.innerHTML = '';
});

test('SegmentedControl exposes Ark radiogroup semantics and selected state', async () => {
  const root = document.createElement('div');
  document.body.append(root);

  const dispose = render(() => {
    const [value, setValue] = createSignal('controlRoom');
    return (
      <div>
        <SegmentedControl
          label="Design Theme"
          items={[
            { label: 'Control Room', value: 'controlRoom' },
            { label: 'Braun', value: 'braun' },
          ]}
          value={value()}
          onValueChange={setValue}
        />
        <button type="button" onClick={() => setValue('braun')}>
          Force Braun
        </button>
      </div>
    );
  }, root);

  const group = screen.getByRole('radiogroup', { name: 'Design Theme' });
  expect(group).toBeVisible();
  expect(group.dataset.scope).toBe('segment-group');
  expect(group.querySelector('[data-part="indicator"]')).not.toBeNull();

  const controlRoom = screen.getByRole('radio', { name: 'Control Room' });
  const braun = screen.getByRole('radio', { name: 'Braun' });
  expect(controlRoom).toHaveAttribute('type', 'radio');
  expect(braun).toHaveAttribute('type', 'radio');
  expect(controlRoom.closest('[data-part="item"]')).toHaveAttribute('data-state', 'checked');
  expect(braun.closest('[data-part="item"]')).toHaveAttribute('data-state', 'unchecked');

  fireEvent.click(screen.getByText('Braun'));
  await waitFor(() =>
    expect(screen.getByText('Braun').closest('[data-part="item"]')).toHaveAttribute(
      'data-state',
      'checked',
    ),
  );
  expect(screen.getByText('Control Room').closest('[data-part="item"]')).toHaveAttribute(
    'data-state',
    'unchecked',
  );

  screen.getByRole('button', { name: 'Force Braun' }).click();
  await waitFor(() =>
    expect(screen.getByText('Braun').closest('[data-part="item"]')).toHaveAttribute(
      'data-state',
      'checked',
    ),
  );
  expect(screen.getByText('Control Room').closest('[data-part="item"]')).toHaveAttribute(
    'data-state',
    'unchecked',
  );

  dispose();
  root.remove();
});

test('SegmentedControl keeps disabled items non-interactive', () => {
  const selected: string[] = [];
  const root = document.createElement('div');
  document.body.append(root);

  const dispose = render(
    () => (
      <SegmentedControl
        label="Color Mode"
        items={[
          { label: 'Light', value: 'light' },
          { label: 'Dark', value: 'dark', disabled: true },
        ]}
        value="light"
        onValueChange={(next) => selected.push(next)}
      />
    ),
    root,
  );

  const dark = screen.getByRole('radio', { name: 'Dark' });
  expect(dark).toBeDisabled();
  expect(screen.getByText('Dark').closest('[data-part="item"]')).toHaveAttribute('data-disabled');
  expect(screen.getByText('Light').closest('[data-part="item"]')).toHaveAttribute(
    'data-state',
    'checked',
  );

  fireEvent.click(screen.getByText('Dark'));
  expect(selected).toEqual([]);
  expect(screen.getByText('Light').closest('[data-part="item"]')).toHaveAttribute(
    'data-state',
    'checked',
  );

  dispose();
  root.remove();
});

test('SegmentedControl moves selection with arrow keys and focuses the target radio', async () => {
  const selected: string[] = [];
  const root = document.createElement('div');
  document.body.append(root);

  const dispose = render(() => {
    const [value, setValue] = createSignal<'controlRoom' | 'braun'>('controlRoom');
    return (
      <SegmentedControl
        label="Design Theme"
        items={[
          { label: 'Control Room', value: 'controlRoom' },
          { label: 'Braun', value: 'braun' },
        ]}
        value={value()}
        onValueChange={(next) => {
          selected.push(next);
          setValue(next);
        }}
      />
    );
  }, root);

  const controlRoom = screen.getByRole('radio', { name: 'Control Room' });
  const braun = screen.getByRole('radio', { name: 'Braun' });

  controlRoom.focus();
  expect(controlRoom).toHaveFocus();

  fireEvent.keyDown(controlRoom, { key: 'ArrowRight' });
  await waitFor(() => expect(selected.at(-1)).toBe('braun'));
  await waitFor(() => expect(braun).toHaveFocus());
  expect(screen.getByText('Braun').closest('[data-part="item"]')).toHaveAttribute(
    'data-state',
    'checked',
  );

  fireEvent.keyDown(braun, { key: 'ArrowLeft' });
  await waitFor(() => expect(selected.at(-1)).toBe('controlRoom'));
  await waitFor(() => expect(controlRoom).toHaveFocus());
  expect(screen.getByText('Control Room').closest('[data-part="item"]')).toHaveAttribute(
    'data-state',
    'checked',
  );

  dispose();
  root.remove();
});

test('SegmentedControl narrow layout contract wraps full-width items without overflow', () => {
  expect(segmentedControlNarrowLayout.rootFlexWrap).toBe('wrap');
  expect(segmentedControlNarrowLayout.rootMinWidth).toBe('[0]');
  expect(segmentedControlNarrowLayout.rootWidth).toBe('full');
  expect(segmentedControlNarrowLayout.itemFlex).toBe('[1 1 9rem]');
  expect(segmentedControlNarrowLayout.itemMinWidth).toBe('[0]');
  expect(segmentedControlNarrowLayout.itemTextOverflow).toBe('ellipsis');
  expect(segmentedControlNarrowLayout.itemTextWhiteSpace).toBe('nowrap');
});
