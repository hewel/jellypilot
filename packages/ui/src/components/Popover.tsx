import type { JSX, ParentProps } from 'solid-js'
import { createEffect, onCleanup, Show, splitProps } from 'solid-js'
import { AnchoredLayerPortal } from '../runtime/AnchoredLayerPortal'
import { createLayerRegistration } from '../runtime/layers'
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
  const layer = createLayerRegistration()
  let root: HTMLDivElement | undefined
  let trigger: HTMLButtonElement | undefined
  let content: HTMLDivElement | undefined

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
      if (event.key === 'Escape' && layer.isTopmost()) setOpen(false, 'escape', event)
    }
    const onPointer = (event: MouseEvent) => {
      if (
        layer.isTopmost() &&
        !root?.contains(event.target as Node) &&
        !content?.contains(event.target as Node)
      ) {
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
        ref={trigger}
        data-part="trigger"
        type="button"
        aria-expanded={local.open}
        aria-haspopup="dialog"
        onClick={(event) => setOpen(!local.open, 'trigger', event)}
      >
        {local.trigger}
      </button>
      <Show when={local.open && layer.portalHost()}>
        <AnchoredLayerPortal mount={layer.portalHost()!} anchor={() => trigger}>
          <div
            ref={(element) => {
              content = element
              layer.mount()
            }}
            data-part="content"
            role="dialog"
            class={popoverContent}
            style={{ position: 'static' }}
          >
            {local.children}
          </div>
        </AnchoredLayerPortal>
      </Show>
    </div>
  )
}
