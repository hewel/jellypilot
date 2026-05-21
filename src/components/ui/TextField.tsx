import { Show, splitProps } from 'solid-js';

type TextFieldVariant = 'filled' | 'outlined';

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
  variant?: TextFieldVariant;
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
  const inputVariantClass = () =>
    variant() === 'outlined' ? 'input-outlined' : 'input-filled';

  return (
    <div class={`group ${local.class ?? ''}`}>
      <label
        for={local.name}
        class="text-label-medium block text-on-surface-variant mb-1 ml-1 uppercase tracking-wider group-focus-within:text-primary transition-colors"
      >
        {local.label}
      </label>
      <input
        id={local.name}
        name={local.name}
        type={local.type ?? 'text'}
        value={local.value}
        onInput={(e) => local.onInput(e.currentTarget.value)}
        onBlur={() => local.onBlur?.()}
        placeholder={local.placeholder}
        disabled={local.disabled}
        class={`${inputVariantClass()} w-full ${local.inputClass ?? ''}`}
        {...rest}
      />
      <Show when={local.error}>
        <p class="text-error text-body-small mt-1.5 ml-1 animate-in slide-in-from-top-1 fade-in duration-200">
          {local.error}
        </p>
      </Show>
      <Show when={local.hint && !local.error}>
        <p class="text-on-surface-variant/70 text-body-small mt-1.5 ml-1">
          {local.hint}
        </p>
      </Show>
    </div>
  );
}
