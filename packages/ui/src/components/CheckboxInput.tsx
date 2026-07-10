import type { ParentProps } from 'solid-js'
import { createEffect, Show, splitProps } from 'solid-js'
import { fieldDescription, fieldError, fieldRoot } from './Field.css'
import { checkboxLabel, checkboxRoot } from './CheckboxInput.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type CheckboxChangeDetails = {
  reason: 'pointer' | 'keyboard'
  event: Event
}

export type CheckboxInputProps = ParentProps<{
  checked: boolean | 'indeterminate'
  name?: string
  value?: string
  label?: string
  description?: string
  error?: string
  required?: boolean
  disabled?: boolean
  class?: string
  id?: string
  onCheckedChange?: (
    next: boolean | 'indeterminate',
    details: CheckboxChangeDetails,
  ) => void
}>

export function CheckboxInput(props: CheckboxInputProps) {
  const [local, rest] = splitProps(props, [
    'checked',
    'name',
    'value',
    'label',
    'description',
    'error',
    'required',
    'disabled',
    'class',
    'id',
    'children',
    'onCheckedChange',
  ])
  const autoId = nextId('check')
  const inputId = () => local.id ?? `jp-${autoId}`
  let inputEl: HTMLInputElement | undefined

  createEffect(() => {
    if (inputEl) {
      inputEl.indeterminate = local.checked === 'indeterminate'
    }
  })

  const emit = (event: Event, reason: CheckboxChangeDetails['reason']) => {
    if (local.disabled) return
    const next =
      local.checked === 'indeterminate' ? true : !Boolean(local.checked)
    local.onCheckedChange?.(next, { reason, event })
  }

  return (
    <div
      data-ui="checkbox-input"
      data-part="root"
      data-state={
        local.checked === 'indeterminate'
          ? 'indeterminate'
          : local.checked
            ? 'checked'
            : 'unchecked'
      }
      class={[fieldRoot, checkboxRoot, local.class].filter(Boolean).join(' ')}
    >
      <label class={checkboxLabel} data-part="label" for={inputId()}>
        <input
          ref={inputEl}
          data-part="control"
          id={inputId()}
          type="checkbox"
          name={local.name}
          value={local.value}
          checked={local.checked === true}
          required={local.required}
          disabled={local.disabled}
          aria-invalid={local.error ? true : undefined}
          onChange={(event) => emit(event, 'pointer')}
          onKeyDown={(event) => {
            if (event.key === ' ' || event.key === 'Enter') {
              event.preventDefault()
              emit(event, 'keyboard')
            }
          }}
          {...rest}
        />
        <span data-part="text">{local.label ?? local.children}</span>
      </label>
      <Show when={local.description}>
        <div class={fieldDescription} data-part="description">
          {local.description}
        </div>
      </Show>
      <Show when={local.error}>
        <div class={fieldError} data-part="error" role="alert">
          {local.error}
        </div>
      </Show>
    </div>
  )
}
