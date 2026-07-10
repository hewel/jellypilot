import type { JSX, ParentProps } from 'solid-js'
import { createEffect, onCleanup, Show, splitProps } from 'solid-js'
import { Portal } from 'solid-js/web'
import { dialogBackdrop, dialogContent, dialogRoot } from './Dialog.css'

export type DialogChangeDetails = {
  reason: 'escape' | 'outside' | 'close-button' | 'controlled'
  event?: Event
}

export type DialogProps = ParentProps<{
  open: boolean
  title: JSX.Element
  description?: JSX.Element
  class?: string
  onOpenChange?: (next: boolean, details: DialogChangeDetails) => void
}>

export function Dialog(props: DialogProps) {
  const [local, rest] = splitProps(props, [
    'open',
    'title',
    'description',
    'class',
    'children',
    'onOpenChange',
  ])
  let panel: HTMLDivElement | undefined

  const close = (reason: DialogChangeDetails['reason'], event?: Event) => {
    local.onOpenChange?.(false, { reason, event })
  }

  createEffect(() => {
    if (!local.open) return
    const previous = document.activeElement as HTMLElement | null
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        close('escape', event)
      }
    }
    document.addEventListener('keydown', onKey)
    panel?.focus()
    onCleanup(() => {
      document.removeEventListener('keydown', onKey)
      previous?.focus?.()
    })
  })

  return (
    <Show when={local.open}>
      <Portal>
        <div data-ui="dialog" data-part="root" class={dialogRoot} {...rest}>
          <div
            data-part="backdrop"
            class={dialogBackdrop}
            onClick={(event) => close('outside', event)}
          />
          <div
            ref={panel}
            data-part="content"
            role="dialog"
            aria-modal="true"
            tabindex="-1"
            class={[dialogContent, local.class].filter(Boolean).join(' ')}
          >
            <h2 data-part="title">{local.title}</h2>
            <Show when={local.description}>
              <p data-part="description">{local.description}</p>
            </Show>
            <div data-part="body">{local.children}</div>
            <button
              data-part="close"
              type="button"
              onClick={(event) => close('close-button', event)}
            >
              Close
            </button>
          </div>
        </div>
      </Portal>
    </Show>
  )
}
