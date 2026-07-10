import type { JSX, ParentProps } from 'solid-js'
import { createEffect, onCleanup, Show, splitProps } from 'solid-js'
import { Portal } from 'solid-js/web'
import { dialogBackdrop, dialogContent, dialogRoot } from './Dialog.css'

export type AlertDialogChangeDetails = {
  reason: 'escape' | 'cancel' | 'action' | 'controlled'
  event?: Event
}

export type AlertDialogProps = ParentProps<{
  open: boolean
  title: JSX.Element
  description?: JSX.Element
  actionLabel?: string
  cancelLabel?: string
  class?: string
  onOpenChange?: (next: boolean, details: AlertDialogChangeDetails) => void
  onAction?: (details: AlertDialogChangeDetails) => void
}>

export function AlertDialog(props: AlertDialogProps) {
  const [local, rest] = splitProps(props, [
    'open',
    'title',
    'description',
    'actionLabel',
    'cancelLabel',
    'class',
    'children',
    'onOpenChange',
    'onAction',
  ])
  let panel: HTMLDivElement | undefined

  const close = (reason: AlertDialogChangeDetails['reason'], event?: Event) => {
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
        <div data-ui="alert-dialog" data-part="root" class={dialogRoot} {...rest}>
          <div data-part="backdrop" class={dialogBackdrop} />
          <div
            ref={panel}
            data-part="content"
            role="alertdialog"
            aria-modal="true"
            tabindex="-1"
            class={[dialogContent, local.class].filter(Boolean).join(' ')}
          >
            <h2 data-part="title">{local.title}</h2>
            <Show when={local.description}>
              <p data-part="description">{local.description}</p>
            </Show>
            <div data-part="body">{local.children}</div>
            <div data-part="actions">
              <button
                data-part="cancel"
                type="button"
                onClick={(event) => close('cancel', event)}
              >
                {local.cancelLabel ?? 'Cancel'}
              </button>
              <button
                data-part="action"
                type="button"
                onClick={(event) => {
                  local.onAction?.({ reason: 'action', event })
                  close('action', event)
                }}
              >
                {local.actionLabel ?? 'Continue'}
              </button>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  )
}
