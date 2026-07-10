import type { JSX, ParentProps } from 'solid-js'
import { Show, splitProps } from 'solid-js'
import {
  fieldControl,
  fieldDescription,
  fieldError,
  fieldLabel,
  fieldRoot,
} from './Field.css'
import type { TextInputChangeDetails } from './TextInput'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type TextAreaProps = ParentProps<{
  value: string
  name?: string
  label?: string
  description?: string
  error?: string
  required?: boolean
  disabled?: boolean
  readOnly?: boolean
  placeholder?: string
  rows?: number
  class?: string
  id?: string
  'aria-label'?: string
  onBlur?: JSX.EventHandlerUnion<HTMLTextAreaElement, FocusEvent>
  onKeyDown?: JSX.EventHandlerUnion<HTMLTextAreaElement, KeyboardEvent>
  onValueChange?: (next: string, details: TextInputChangeDetails) => void
}>

export function TextArea(props: TextAreaProps) {
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
    'rows',
    'class',
    'id',
    'children',
    'onValueChange',
  ])
  const autoId = nextId('textarea')
  const inputId = () => local.id ?? `jp-${autoId}`
  const descId = () => `${inputId()}-desc`
  const errId = () => `${inputId()}-err`

  return (
    <div data-ui="text-area" data-part="root" class={fieldRoot}>
      <Show when={local.label}>
        <label class={fieldLabel} data-part="label" for={inputId()}>
          {local.label}
          <Show when={local.required}>
            <span aria-hidden="true"> *</span>
          </Show>
        </label>
      </Show>
      <textarea
        data-part="control"
        id={inputId()}
        class={[fieldControl, local.class].filter(Boolean).join(' ')}
        name={local.name}
        value={local.value}
        placeholder={local.placeholder}
        rows={local.rows ?? 3}
        required={local.required}
        disabled={local.disabled}
        readOnly={local.readOnly}
        aria-invalid={local.error ? true : undefined}
        aria-describedby={
          [local.description ? descId() : null, local.error ? errId() : null]
            .filter(Boolean)
            .join(' ') || undefined
        }
        onInput={(event) =>
          local.onValueChange?.(event.currentTarget.value, {
            reason: 'input',
            event,
          })
        }
        onChange={(event) =>
          local.onValueChange?.(event.currentTarget.value, {
            reason: 'change',
            event,
          })
        }
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
