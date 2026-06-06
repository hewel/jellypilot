import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { JmsrSelect } from '../src/components/ui';

Element.prototype.scrollTo = () => {};

afterEach(() => {
  document.body.innerHTML = '';
});

test('JmsrSelect renders an Ark select and emits the selected value', async () => {
  const selectedValues: string[] = [];
  const root = document.createElement('div');
  document.body.append(root);

  const dispose = render(() => {
    const [value, setValue] = createSignal('eng');

    return (
      <JmsrSelect
        label="Subtitle language"
        items={[
          { value: 'eng', label: 'eng - English' },
          { value: 'jpn', label: 'jpn - Japanese' },
        ]}
        value={value()}
        placeholder="Select a language..."
        onValueChange={(nextValue) => {
          selectedValues.push(nextValue);
          setValue(nextValue);
        }}
      />
    );
  }, root);

  const trigger = screen.getByRole('combobox', {
    name: 'Subtitle language',
  });
  expect(trigger.closest('[data-scope="select"]')).not.toBeNull();
  expect(trigger).toHaveTextContent('eng - English');
  expect(
    screen
      .getAllByLabelText('Subtitle language')
      .find((element) => element.tagName === 'SELECT'),
  ).toHaveValue('eng');

  fireEvent.click(trigger);
  fireEvent.click(
    await screen.findByRole('option', { name: 'jpn - Japanese' }),
  );

  await waitFor(() => expect(selectedValues).toEqual(['jpn']));
  expect(trigger).toHaveTextContent('jpn - Japanese');

  dispose();
  root.remove();
});
