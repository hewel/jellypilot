import { afterEach, expect, test } from '@rstest/core';
import { fireEvent, screen, waitFor } from '@testing-library/dom';
import { createSignal } from 'solid-js';
import { render } from 'solid-js/web';

import { JellyPilotSelect } from '../src/components/ui';

Element.prototype.scrollTo = () => {};

afterEach(() => {
  document.body.innerHTML = '';
});

test('JellyPilotSelect renders a local select and emits the selected value', async () => {
  const selectedValues: string[] = [];
  const root = document.createElement('div');
  document.body.append(root);

  const dispose = render(() => {
    const [value, setValue] = createSignal('eng');

    return (
      <JellyPilotSelect
        label="Subtitle language"
        items={[
          { label: 'eng - English', value: 'eng' },
          { label: 'jpn - Japanese', value: 'jpn' },
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
  expect(trigger).toHaveTextContent('eng - English');
  expect(document.querySelector('[data-native-select="Subtitle language"]')).toHaveValue('eng');

  fireEvent.click(trigger);
  fireEvent.click(await screen.findByRole('option', { name: 'jpn - Japanese' }));

  await waitFor(() => expect(selectedValues).toEqual(['jpn']));
  expect(trigger).toHaveTextContent('jpn - Japanese');

  dispose();
  root.remove();
});
