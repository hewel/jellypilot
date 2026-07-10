import type { JSX, ParentProps } from 'solid-js'
import { createEffect, onCleanup, Show, splitProps } from 'solid-js'
import { Portal } from 'solid-js/web'
import { popoverContent, popoverRoot } from './Popover.css'

export type PopoverChangeDetails = {
  reason: 'trigger' | 'escape' | 'outside' | 'controlled'
  event?: Event
}

export type PopoverProps = ParentProps<{
  open: boolean
  trigger: JSX.Element
  class?: string
  onOpenChange?: (next: boolean, details: PopoverChangeDetails) => void
}>

export function Popover(props: PopoverProps) {
  const [local, rest] = splitProps(props, [
    'open',
    'trigger',
    'class',
    'children',
    'onOpenChange',
  ])
  let root: HTMLDivElement | undefined

  const setOpen = (
    next: boolean,
    reason: PopoverChangeDetails['reason'],
    event?: Event,
  ) => {
    local.onOpenChange?.(next, { reason, event })
  }

  createEffect(() => {
    if (!local.open) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false, 'escape', event)
    }
    const onPointer = (event: MouseEvent) => {
      if (!root?.contains(event.target as Node)) {
        setOpen(false, 'outside', event)
      }
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('mousedown', onPointer)
    onCleanup(() => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('mousedown', onPointer)
    })
  })

  return (
    <div
      ref={root}
      data-ui="popover"
      data-part="root"
      data-state={local.open ? 'open' : 'closed'}
      class={[popoverRoot, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      <button
        data-part="trigger"
        type="button"
        aria-expanded={local.open}
        aria-haspopup="dialog"
        onClick={(event) => setOpen(!local.open, 'trigger', event)}
      >
        {local.trigger}
      </button>
      <Show when={local.open}>
        <Portal>
          <div data-part="content" role="dialog" class={popoverContent}>
            {local.children}
          </div>
        </Portal>
      </Show>
    </div>
  )
}
