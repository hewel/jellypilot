import type { JSX, ParentProps } from 'solid-js'
import { Show, splitProps } from 'solid-js'

export type CollapsibleChangeDetails = {
  reason: 'pointer' | 'keyboard'
  event: Event
}

export type CollapsibleProps = ParentProps<{
  open: boolean
  disabled?: boolean
  class?: string
  trigger: JSX.Element
  onOpenChange?: (next: boolean, details: CollapsibleChangeDetails) => void
}>

export function Collapsible(props: CollapsibleProps) {
  const [local, rest] = splitProps(props, [
    'open',
    'disabled',
    'class',
    'trigger',
    'children',
    'onOpenChange',
  ])

  const toggle = (event: Event, reason: CollapsibleChangeDetails['reason']) => {
    if (local.disabled) return
    local.onOpenChange?.(!local.open, { reason, event })
  }

  return (
    <div
      data-ui="collapsible"
      data-part="root"
      data-state={local.open ? 'open' : 'closed'}
      class={local.class}
      {...rest}
    >
      <button
        data-part="trigger"
        type="button"
        aria-expanded={local.open}
        disabled={local.disabled}
        onClick={(event) => toggle(event, 'pointer')}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            toggle(event, 'keyboard')
          }
        }}
      >
        {local.trigger}
      </button>
      <Show when={local.open}>
        <div data-part="content">{local.children}</div>
      </Show>
    </div>
  )
}
