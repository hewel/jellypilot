import { SegmentGroup } from '@ark-ui/solid/segment-group';
import { cx } from '@styled-system/css';
import { For } from 'solid-js';

import * as styles from './SegmentedControl.styles';

export interface SegmentedControlItem<Value extends string = string> {
  value: Value;
  label: string;
  disabled?: boolean;
}

interface SegmentedControlProps<Value extends string = string> {
  label: string;
  items: readonly SegmentedControlItem<Value>[];
  value: Value;
  onValueChange: (value: Value) => void;
  disabled?: boolean;
  class?: string;
}

export default function SegmentedControl<Value extends string>(
  props: SegmentedControlProps<Value>,
) {
  let rootEl: HTMLDivElement | undefined;

  const focusValue = (value: Value) => {
    const root = rootEl;
    if (!root) return;
    const target = root.querySelector<HTMLInputElement>(
      `input[type="radio"][value="${CSS.escape(value)}"]:not(:disabled)`,
    );
    target?.focus();
  };

  const moveSelection = (key: string) => {
    if (props.disabled) return false;

    const items = props.items.filter((item) => !item.disabled);
    if (items.length === 0) return false;

    const currentIndex = items.findIndex((item) => item.value === props.value);
    const forward = key === 'ArrowRight' || key === 'ArrowDown';
    const backward = key === 'ArrowLeft' || key === 'ArrowUp';
    if (!forward && !backward) return false;

    let nextIndex = currentIndex;
    if (currentIndex === -1) {
      nextIndex = forward ? 0 : items.length - 1;
    } else if (forward) {
      nextIndex = (currentIndex + 1) % items.length;
    } else {
      nextIndex = (currentIndex - 1 + items.length) % items.length;
    }

    const next = items[nextIndex];
    if (!next) return false;
    if (next.value !== props.value) {
      props.onValueChange(next.value);
    }
    // Defer focus until after the controlled selected state commits.
    requestAnimationFrame(() => focusValue(next.value));
    return true;
  };

  return (
    <SegmentGroup.Root
      ref={(el) => {
        rootEl = el;
      }}
      class={cx(styles.root, props.class)}
      value={props.value}
      disabled={props.disabled}
      orientation="horizontal"
      onValueChange={(details) => {
        const next = details.value;
        if (next == null) return;
        const item = props.items.find((candidate) => candidate.value === next);
        if (!item || item.disabled) return;
        props.onValueChange(item.value);
      }}
      onKeyDown={(event) => {
        if (
          event.key !== 'ArrowRight' &&
          event.key !== 'ArrowLeft' &&
          event.key !== 'ArrowDown' &&
          event.key !== 'ArrowUp'
        ) {
          return;
        }
        if (!moveSelection(event.key)) return;
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <SegmentGroup.Label class={styles.label}>{props.label}</SegmentGroup.Label>
      <div class={styles.track}>
        <SegmentGroup.Indicator class={styles.indicator} />
        <For each={props.items}>
          {(item) => (
            <SegmentGroup.Item
              value={item.value}
              disabled={Boolean(item.disabled || props.disabled)}
              class={styles.item()}
            >
              <SegmentGroup.ItemText class={styles.itemText}>{item.label}</SegmentGroup.ItemText>
              <SegmentGroup.ItemControl class={styles.itemControl} />
              <SegmentGroup.ItemHiddenInput class={styles.hiddenInput} />
            </SegmentGroup.Item>
          )}
        </For>
      </div>
    </SegmentGroup.Root>
  );
}
