import { createListCollection } from '@ark-ui/solid/collection';
import { Select } from '@ark-ui/solid/select';
import { ChevronDown } from 'lucide-solid';
import { For, createMemo } from 'solid-js';
import { Portal } from 'solid-js/web';

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
  const labelClass = () =>
    isCompact()
      ? 'mb-2 block text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase text-on-surface-variant'
      : 'mb-1.5 block text-on-surface-variant text-[12px] leading-[16px] font-bold tracking-[0.05em] uppercase';
  const triggerClass = () =>
    isCompact()
      ? 'flex h-12 w-full items-center justify-between gap-2 rounded-lg border border-outline-variant bg-surface-container-low px-3 text-left text-on-surface outline-none transition-all duration-200 hover:border-secondary/50 focus:border-secondary focus:ring-2 focus:ring-secondary/25 disabled:cursor-not-allowed disabled:opacity-50'
      : 'flex h-14 w-full items-center justify-between gap-2 rounded-2xl border border-outline-variant/80 bg-surface-container-highest/30 px-4 text-left text-on-surface outline-none transition-all duration-200 hover:border-secondary/50 focus:border-secondary focus:ring-4 focus:ring-secondary/15 disabled:cursor-not-allowed disabled:opacity-50';
  const SelectContent = () => (
    <Select.Positioner>
      <Select.Content class="border-outline-variant bg-surface-container-lowest max-h-60 overflow-y-auto rounded-lg border p-2 shadow-2xl backdrop-blur-md">
        <For each={collection().items}>
          {(item) => (
            <Select.Item
              item={item}
              class="text-on-surface-variant hover:bg-surface-container-high hover:text-on-surface flex cursor-pointer items-center justify-between rounded-xl px-3.5 py-2.5 text-[14px] leading-[20px] transition-colors data-disabled:cursor-not-allowed data-disabled:opacity-50"
            >
              <Select.ItemText class="font-medium">{item.label}</Select.ItemText>
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
      <Select.Label class={labelClass()}>{props.label}</Select.Label>
      <Select.Control class="flex w-full items-center">
        <Select.Trigger class={triggerClass()}>
          <Select.ValueText
            placeholder={props.placeholder}
            class="text-on-surface min-w-0 truncate text-[14px] leading-[20px] font-medium"
          />
          <Select.Indicator>
            <ChevronDown class="text-on-surface-variant/70 h-4 w-4" />
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
