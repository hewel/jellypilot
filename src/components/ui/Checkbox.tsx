import { Show, createSignal } from 'solid-js';
import type { JSX } from 'solid-js';

import * as styles from './Checkbox.css';

export interface CheckboxProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  children: JSX.Element;
  indicator?: JSX.Element;
  class?: string;
  controlClass?: string;
  indicatorClass?: string;
  labelClass?: string;
  disabled?: boolean;
}

export default function Checkbox(props: CheckboxProps) {
  const [focusVisible, setFocusVisible] = createSignal(false);
  const state = () => (props.checked ? 'checked' : 'unchecked');
  const disabled = () => props.disabled === true;

  return (
    <label class={props.class} data-state={state()} data-disabled={disabled() ? '' : undefined}>
      <input
        type="checkbox"
        class={styles.input}
        checked={props.checked}
        disabled={disabled()}
        onChange={(event) => props.onCheckedChange(event.currentTarget.checked)}
        onFocus={(event) => setFocusVisible(event.currentTarget.matches(':focus-visible'))}
        onBlur={() => setFocusVisible(false)}
      />
      <span
        class={props.controlClass}
        data-state={state()}
        data-disabled={disabled() ? '' : undefined}
        data-focus-visible={focusVisible() ? '' : undefined}
        aria-hidden="true"
      >
        <Show when={props.checked}>
          <span class={props.indicatorClass}>{props.indicator}</span>
        </Show>
      </span>
      <span class={props.labelClass}>{props.children}</span>
    </label>
  );
}
