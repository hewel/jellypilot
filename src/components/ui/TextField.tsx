import { Show, splitProps } from 'solid-js';

import { FieldControl } from './FieldControl';
import type { FieldControlVariant } from './FieldControl';
import * as styles from './TextField.styles';

interface TextFieldProps {
  name: string;
  label: string;
  value: string;
  onInput: (value: string) => void;
  onBlur?: () => void;
  type?: 'text' | 'password' | 'url';
  placeholder?: string;
  disabled?: boolean;
  error?: string;
  hint?: string;
  variant?: FieldControlVariant;
  class?: string;
  inputClass?: string;
}

/**
 * Control Room text field with label, input, error, and hint.
 */
export default function TextField(props: TextFieldProps) {
  const [local, rest] = splitProps(props, [
    'name',
    'label',
    'value',
    'onInput',
    'onBlur',
    'type',
    'placeholder',
    'disabled',
    'error',
    'hint',
    'variant',
    'class',
    'inputClass',
  ]);

  const variant = () => local.variant ?? 'filled';

  return (
    <div class={styles.cx(styles.textFieldRoot, 'group', local.class)}>
      <label for={local.name} class={styles.textFieldLabel}>
        {local.label}
      </label>
      <FieldControl
        id={local.name}
        name={local.name}
        type={local.type ?? 'text'}
        value={local.value}
        onInput={(e) => local.onInput(e.currentTarget.value)}
        onBlur={() => local.onBlur?.()}
        placeholder={local.placeholder}
        disabled={local.disabled}
        variant={variant()}
        class={styles.cx(styles.fullWidth, local.inputClass)}
        {...rest}
      />
      <Show when={local.error}>
        <p class={styles.textFieldError}>{local.error}</p>
      </Show>
      <Show when={local.hint && !local.error}>
        <p class={styles.textFieldHint}>{local.hint}</p>
      </Show>
    </div>
  );
}
