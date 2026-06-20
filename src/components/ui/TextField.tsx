import { Show, splitProps } from 'solid-js';

import { FieldControl } from './FieldControl';
import type { FieldControlVariant } from './FieldControl';

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
    <div class={`group ${local.class ?? ''}`}>
      <label
        for={local.name}
        class="text-on-surface-variant group-focus-within:text-primary mb-1 ml-1 block text-[12px] leading-[16px] font-bold tracking-[0.05em] tracking-wider uppercase transition-colors"
      >
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
        class={`w-full ${local.inputClass ?? ''}`}
        {...rest}
      />
      <Show when={local.error}>
        <p class="text-error animate-in slide-in-from-top-1 fade-in mt-1.5 ml-1 text-[12px] leading-[16px] duration-200">
          {local.error}
        </p>
      </Show>
      <Show when={local.hint && !local.error}>
        <p class="text-on-surface-variant/70 mt-1.5 ml-1 text-[12px] leading-[16px]">
          {local.hint}
        </p>
      </Show>
    </div>
  );
}
