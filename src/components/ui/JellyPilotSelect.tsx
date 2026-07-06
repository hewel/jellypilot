import { createListCollection } from '@ark-ui/solid/collection';
import { Select } from '@ark-ui/solid/select';
import { ChevronDown } from 'lucide-solid';
import { For, createMemo } from 'solid-js';
import { Portal } from 'solid-js/web';

import * as styles from './JellyPilotSelect.css';

export interface JellyPilotSelectItem<Value extends string = string> {
  value: Value;
  label: string;
  disabled?: boolean;
}

type JellyPilotSelectSize = 'standard' | 'compact';

interface JellyPilotSelectProps<Value extends string = string> {
  label: string;
  items: JellyPilotSelectItem<Value>[];
  value: Value | null;
  onValueChange: (value: Value) => void;
  placeholder?: string;
  disabled?: boolean;
  size?: JellyPilotSelectSize;
  portalMount?: HTMLElement;
  class?: string;
}

export default function JellyPilotSelect<Value extends string>(
  props: JellyPilotSelectProps<Value>,
) {
  const collection = createMemo(() => createListCollection({ items: props.items }));
  const selectedValue = () => (props.value === null ? [] : [props.value]);
  const isCompact = () => props.size === 'compact';
  const SelectContent = () => (
    <Select.Positioner>
      <Select.Content class={styles.content}>
        <For each={collection().items}>
          {(item) => (
            <Select.Item item={item} class={styles.item}>
              <Select.ItemText class={styles.itemText}>{item.label}</Select.ItemText>
            </Select.Item>
          )}
        </For>
      </Select.Content>
    </Select.Positioner>
  );

  return (
    <Select.Root
      collection={collection()}
      closeOnSelect
      disabled={props.disabled}
      value={selectedValue()}
      onValueChange={(details) => {
        const value = details.value[0];
        const item = props.items.find((candidate) => candidate.value === value);
        if (item && !item.disabled) {
          props.onValueChange(item.value);
        }
      }}
      class={props.class}
      positioning={{ sameWidth: true }}
    >
      <Select.Label class={styles.label({ size: isCompact() ? 'compact' : 'standard' })}>
        {props.label}
      </Select.Label>
      <Select.Control class={styles.control}>
        <Select.Trigger class={styles.trigger({ size: isCompact() ? 'compact' : 'standard' })}>
          <Select.ValueText
            placeholder={props.placeholder}
            class={`${styles.valueText} ${styles.truncate}`}
          />
          <Select.Indicator class={styles.indicator}>
            <ChevronDown class={styles.indicatorIcon} />
          </Select.Indicator>
        </Select.Trigger>
      </Select.Control>
      <Portal mount={props.portalMount}>
        <SelectContent />
      </Portal>
      <Select.HiddenSelect />
    </Select.Root>
  );
}
