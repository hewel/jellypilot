import type { JSX, ParentProps } from 'solid-js'
import { createEffect, onCleanup, Show, splitProps } from 'solid-js'
import { Portal } from 'solid-js/web'
import { dialogBackdrop, dialogContent, dialogRoot } from './Dialog.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type AlertDialogChangeDetails = {
  reason: 'escape' | 'outside' | 'cancel' | 'action' | 'controlled'
  event?: Event
}

export type AlertDialogProps = ParentProps<{
  open: boolean
  title: JSX.Element
  description?: JSX.Element
  actionLabel?: string
  cancelLabel?: string
  dismissable?: boolean
  closeOnAction?: boolean
  actionDisabled?: boolean
  cancelDisabled?: boolean
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
    'dismissable',
    'closeOnAction',
    'actionDisabled',
    'cancelDisabled',
    'class',
    'children',
    'onOpenChange',
    'onAction',
  ])
  const autoId = nextId('alert-dialog')
  const titleId = () => `${autoId}-title`
  const descriptionId = () => `${autoId}-description`
  let panel: HTMLDivElement | undefined

  const close = (reason: AlertDialogChangeDetails['reason'], event?: Event) => {
    local.onOpenChange?.(false, { reason, event })
  }

  const dismissable = () => local.dismissable ?? true

  createEffect(() => {
    if (!local.open) return
    const previous = document.activeElement as HTMLElement | null
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && dismissable()) {
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
          <div
            data-part="backdrop"
            class={dialogBackdrop}
            onClick={(event) => {
              if (dismissable()) close('outside', event)
            }}
          />
          <div
            ref={panel}
            data-part="content"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby={titleId()}
            aria-describedby={local.description ? descriptionId() : undefined}
            tabindex="-1"
            class={[dialogContent, local.class].filter(Boolean).join(' ')}
            onKeyDown={(event) => {
              if (event.key === 'Escape' && dismissable()) {
                event.stopPropagation()
                close('escape', event)
              }
            }}
          >
            <h2 data-part="title" id={titleId()}>
              {local.title}
            </h2>
            <Show when={local.description}>
              <p data-part="description" id={descriptionId()}>
                {local.description}
              </p>
            </Show>
            <div data-part="body">{local.children}</div>
            <div data-part="actions">
              <button
                data-part="cancel"
                type="button"
                disabled={local.cancelDisabled}
                onClick={(event) => close('cancel', event)}
              >
                {local.cancelLabel ?? 'Cancel'}
              </button>
              <button
                data-part="action"
                type="button"
                disabled={local.actionDisabled}
                onClick={(event) => {
                  local.onAction?.({ reason: 'action', event })
                  if (local.closeOnAction ?? true) close('action', event)
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
