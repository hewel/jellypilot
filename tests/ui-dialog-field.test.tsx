import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

import { Dialog, Field } from '../src/components/ui';

test('Dialog opens from its trigger, exposes label and description, and closes on Escape', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => {
    const [open, setOpen] = createSignal(false);
    return (
      <Dialog.Root open={open()} onOpenChange={(details) => setOpen(details.open)}>
        <Dialog.Trigger>Open panel</Dialog.Trigger>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Title>Playback settings</Dialog.Title>
            <Dialog.Description>Configure player behavior.</Dialog.Description>
            <Dialog.CloseTrigger>Close</Dialog.CloseTrigger>
          </Dialog.Content>
        </Dialog.Positioner>
      </Dialog.Root>
    );
  }, root);

  fireEvent.click(screen.getByRole('button', { name: 'Open panel' }));
  const dialog = await screen.findByRole('dialog', { name: 'Playback settings' });
  expect(dialog).toHaveAccessibleDescription('Configure player behavior.');

  fireEvent.keyDown(dialog, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());

  dispose();
  root.remove();
});

test('Field links label, helper text, error text, disabled state, and invalid state', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(
    () => (
      <Field.Root disabled invalid>
        <Field.Label>Server URL</Field.Label>
        <Field.Input />
        <Field.HelperText>Use your media server address.</Field.HelperText>
        <Field.ErrorText>Server URL is required.</Field.ErrorText>
      </Field.Root>
    ),
    root,
  );

  const input = screen.getByLabelText('Server URL');
  expect(input).toBeDisabled();
  expect(input).toHaveAttribute('aria-invalid', 'true');
  expect(input).toHaveAccessibleDescription(
    'Use your media server address. Server URL is required.',
  );

  dispose();
  root.remove();
});

afterEach(() => {
  document.body.innerHTML = '';
});
