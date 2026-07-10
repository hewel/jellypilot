import type { ParentProps } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import {
  fieldControl,
  fieldDescription,
  fieldError,
  fieldLabel,
  fieldRoot,
} from './Field.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type TextInputChangeDetails = {
  reason: 'input' | 'change'
  event: Event
}

export type TextInputProps = ParentProps<{
  value: string
  name?: string
  label?: string
  description?: string
  error?: string
  required?: boolean
  disabled?: boolean
  readOnly?: boolean
  placeholder?: string
  type?: string
  class?: string
  id?: string
  onValueChange?: (next: string, details: TextInputChangeDetails) => void
}>

export function TextInput(props: TextInputProps) {
  const [local, rest] = splitProps(props, [
    'value',
    'name',
    'label',
    'description',
    'error',
    'required',
    'disabled',
    'readOnly',
    'placeholder',
    'type',
    'class',
    'id',
    'children',
    'onValueChange',
  ])
  const autoId = nextId('text')
  const inputId = () => local.id ?? `jp-${autoId}`
  const descId = () => `${inputId()}-desc`
  const errId = () => `${inputId()}-err`

  const emit = (
    next: string,
    reason: TextInputChangeDetails['reason'],
    event: Event,
  ) => {
    local.onValueChange?.(next, { reason, event })
  }

  return (
    <div data-ui="text-input" data-part="root" class={fieldRoot}>
      <Show when={local.label}>
        <label class={fieldLabel} data-part="label" for={inputId()}>
          {local.label}
          <Show when={local.required}>
            <span aria-hidden="true"> *</span>
          </Show>
        </label>
      </Show>
      <input
        data-part="control"
        id={inputId()}
        class={[fieldControl, local.class].filter(Boolean).join(' ')}
        type={local.type ?? 'text'}
        name={local.name}
        value={local.value}
        placeholder={local.placeholder}
        required={local.required}
        disabled={local.disabled}
        readOnly={local.readOnly}
        aria-invalid={local.error ? true : undefined}
        aria-describedby={
          [local.description ? descId() : null, local.error ? errId() : null]
            .filter(Boolean)
            .join(' ') || undefined
        }
        onInput={(event) => emit(event.currentTarget.value, 'input', event)}
        onChange={(event) => emit(event.currentTarget.value, 'change', event)}
        {...rest}
      />
      <Show when={local.description}>
        <div class={fieldDescription} data-part="description" id={descId()}>
          {local.description}
        </div>
      </Show>
      <Show when={local.error}>
        <div class={fieldError} data-part="error" id={errId()} role="alert">
          {local.error}
        </div>
      </Show>
    </div>
  )
}
