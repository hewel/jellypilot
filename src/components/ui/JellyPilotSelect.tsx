import { ChevronDown } from 'lucide-solid';
import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  onCleanup,
} from 'solid-js';
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
  const [open, setOpen] = createSignal(false);
  const [position, setPosition] = createSignal({ left: 0, top: 0, width: 0 });
  const rootId = createUniqueId();
  const labelId = `${rootId}-label`;
  const listboxId = `${rootId}-listbox`;
  const isCompact = () => props.size === 'compact';
  const selectedItem = createMemo(() =>
    props.value === null ? null : (props.items.find((item) => item.value === props.value) ?? null),
  );
  const enabledItems = createMemo(() => props.items.filter((item) => !item.disabled));
  const activeIndex = () => {
    const index = enabledItems().findIndex((item) => item.value === props.value);
    return index === -1 ? 0 : index;
  };
  let rootRef: HTMLDivElement | undefined;
  let triggerRef: HTMLButtonElement | undefined;

  const updatePosition = () => {
    if (!triggerRef) {
      return;
    }
    const rect = triggerRef.getBoundingClientRect();
    setPosition({
      left: rect.left,
      top: rect.bottom + 4,
      width: rect.width,
    });
  };

  createEffect(() => {
    if (!open()) {
      return;
    }

    updatePosition();
    const handlePointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && rootRef?.contains(event.target)) {
        return;
      }
      setOpen(false);
    };

    document.addEventListener('pointerdown', handlePointerDown);
    onCleanup(() => document.removeEventListener('pointerdown', handlePointerDown));
  });

  const chooseItem = (item: JellyPilotSelectItem<Value>) => {
    if (item.disabled) {
      return;
    }
    props.onValueChange(item.value);
    setOpen(false);
  };

  const chooseByOffset = (offset: number) => {
    const items = enabledItems();
    if (items.length === 0) {
      return;
    }
    const nextIndex = (activeIndex() + offset + items.length) % items.length;
    chooseItem(items[nextIndex]);
  };

  const handleTriggerKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      setOpen(false);
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      setOpen((current) => !current);
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (!open()) {
        setOpen(true);
        return;
      }
      chooseByOffset(1);
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (!open()) {
        setOpen(true);
        return;
      }
      chooseByOffset(-1);
    }
  };

  return (
    <div ref={rootRef} class={props.class}>
      <label id={labelId} class={styles.label({ size: isCompact() ? 'compact' : 'standard' })}>
        {props.label}
      </label>
      <div class={styles.control}>
        <button
          ref={triggerRef}
          type="button"
          id={`${rootId}-trigger`}
          class={styles.trigger({ size: isCompact() ? 'compact' : 'standard' })}
          role="combobox"
          aria-controls={listboxId}
          aria-expanded={open()}
          aria-haspopup="listbox"
          aria-labelledby={labelId}
          disabled={props.disabled}
          onClick={() => setOpen((current) => !current)}
          onKeyDown={handleTriggerKeyDown}
        >
          <span class={`${styles.valueText} ${styles.truncate}`}>
            {selectedItem()?.label ?? props.placeholder ?? ''}
          </span>
          <span class={styles.indicator} data-state={open() ? 'open' : 'closed'}>
            <ChevronDown class={styles.indicatorIcon} />
          </span>
        </button>
      </div>
      <Portal mount={props.portalMount}>
        <Show when={open()}>
          <div
            class={styles.positioner}
            style={{
              left: `${position().left}px`,
              top: `${position().top}px`,
              width: `${position().width}px`,
            }}
          >
            <div id={listboxId} role="listbox" aria-labelledby={labelId} class={styles.content}>
              <For each={props.items}>
                {(item) => (
                  <button
                    type="button"
                    role="option"
                    aria-selected={item.value === props.value}
                    disabled={item.disabled}
                    data-disabled={item.disabled ? '' : undefined}
                    class={styles.item}
                    onClick={() => chooseItem(item)}
                  >
                    <span class={styles.itemText}>{item.label}</span>
                  </button>
                )}
              </For>
            </div>
          </div>
        </Show>
      </Portal>
      <select
        id={`${rootId}-native`}
        aria-hidden="true"
        class={styles.hiddenSelect}
        data-native-select={props.label}
        disabled={props.disabled}
        tabIndex={-1}
        value={props.value ?? ''}
        onChange={(event) => {
          const value = event.currentTarget.value;
          const item = props.items.find((candidate) => candidate.value === value);
          if (item && !item.disabled) {
            props.onValueChange(item.value);
          }
        }}
      >
        <Show when={props.placeholder || props.value === null}>
          <option value="">{props.placeholder ?? ''}</option>
        </Show>
        <For each={props.items}>
          {(item) => (
            <option value={item.value} disabled={item.disabled}>
              {item.label}
            </option>
          )}
        </For>
      </select>
    </div>
  );
}
