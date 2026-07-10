import type { ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import {
  buttonBase,
  buttonOutline,
  buttonMd,
  togglePressed,
} from './ActionControl.css'

export type ToggleButtonChangeDetails = {
  reason: 'pointer' | 'keyboard'
  event: Event
}

export type ToggleButtonProps = ParentProps<{
  pressed: boolean
  disabled?: boolean
  class?: string
  'aria-label'?: string
  onPressedChange?: (
    next: boolean,
    details: ToggleButtonChangeDetails,
  ) => void
}>

export function ToggleButton(props: ToggleButtonProps) {
  const [local, rest] = splitProps(props, [
    'pressed',
    'disabled',
    'class',
    'children',
    'onPressedChange',
  ])

  const emit = (next: boolean, reason: 'pointer' | 'keyboard', event: Event) => {
    local.onPressedChange?.(next, { reason, event })
  }

  return (
    <button
      data-ui="toggle-button"
      data-part="root"
      data-pressed={local.pressed ? 'true' : 'false'}
      type="button"
      aria-pressed={local.pressed}
      disabled={local.disabled}
      class={[buttonBase, buttonOutline, buttonMd, local.class]
        .filter(Boolean)
        .join(' ')}
      classList={{ [togglePressed]: local.pressed }}
      onClick={(event) => {
        if (local.disabled) return
        emit(!local.pressed, 'pointer', event)
      }}
      onKeyDown={(event) => {
        if (local.disabled) return
        if (event.key === ' ' || event.key === 'Enter') {
          event.preventDefault()
          emit(!local.pressed, 'keyboard', event)
        }
      }}
      {...rest}
    >
      {local.children}
    </button>
  )
}
