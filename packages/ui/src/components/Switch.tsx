import type { ParentProps } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import { fieldDescription, fieldError, fieldRoot } from './Field.css'
import { switchControl, switchRoot, switchThumb } from './Switch.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type SwitchChangeDetails = {
  reason: 'pointer' | 'keyboard'
  event: Event
}

export type SwitchProps = ParentProps<{
  checked: boolean
  name?: string
  label?: string
  description?: string
  error?: string
  required?: boolean
  disabled?: boolean
  class?: string
  id?: string
  onCheckedChange?: (next: boolean, details: SwitchChangeDetails) => void
}>

export function Switch(props: SwitchProps) {
  const [local, rest] = splitProps(props, [
    'checked',
    'name',
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
  const autoId = nextId('switch')
  const inputId = () => local.id ?? `jp-${autoId}`

  const emit = (event: Event, reason: SwitchChangeDetails['reason']) => {
    if (local.disabled) return
    local.onCheckedChange?.(!local.checked, { reason, event })
  }

  return (
    <div
      data-ui="switch"
      data-part="root"
      data-state={local.checked ? 'checked' : 'unchecked'}
      class={[fieldRoot, switchRoot, local.class].filter(Boolean).join(' ')}
    >
      <label for={inputId()} data-part="label">
        <button
          data-part="control"
          id={inputId()}
          type="button"
          role="switch"
          aria-checked={local.checked}
          aria-required={local.required || undefined}
          disabled={local.disabled}
          class={switchControl}
          onClick={(event) => emit(event, 'pointer')}
          onKeyDown={(event) => {
            if (event.key === ' ' || event.key === 'Enter') {
              event.preventDefault()
              emit(event, 'keyboard')
            }
          }}
          {...rest}
        >
          <span data-part="thumb" class={switchThumb} />
        </button>
        <Show when={local.name}>
          <input
            type="checkbox"
            name={local.name}
            checked={local.checked}
            hidden
            tabIndex={-1}
            readOnly
          />
        </Show>
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
