import type { JSX, ParentProps } from 'solid-js'
import { createEffect, createUniqueId, onCleanup, Show, splitProps } from 'solid-js'
import { focusFirst, trapTabKey } from '../runtime/focus'
import { LayerPortal } from '../runtime/LayerPortal'
import { createLayerRegistration } from '../runtime/layers'
import { dialogBackdrop, dialogContent, dialogRoot } from './DialogLayer.css'

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
  const autoId = createUniqueId()
  const titleId = () => `${autoId}-title`
  const descriptionId = () => `${autoId}-description`
  const layer = createLayerRegistration({ modal: true })
  let panel: HTMLDivElement | undefined

  const close = (reason: AlertDialogChangeDetails['reason'], event?: Event) => {
    local.onOpenChange?.(false, { reason, event })
  }

  const dismissable = () => local.dismissable ?? true

  createEffect(() => {
    if (!local.open) return
    const previous = document.activeElement as HTMLElement | null
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && dismissable() && layer.isTopmost()) {
        event.stopPropagation()
        close('escape', event)
      }
    }
    const onFocusIn = (event: FocusEvent) => {
      if (!panel || !layer.isTopmost() || panel.contains(event.target as Node)) return
      focusFirst(panel)
      if (!panel.contains(document.activeElement)) panel.focus()
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('focusin', onFocusIn)
    queueMicrotask(() => {
      if (!panel || !layer.isTopmost()) return
      focusFirst(panel)
      if (!panel.contains(document.activeElement)) panel.focus()
    })
    onCleanup(() => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('focusin', onFocusIn)
      queueMicrotask(() => previous?.focus?.())
    })
  })

  return (
    <Show when={local.open && layer.portalHost()}>
      <LayerPortal mount={layer.portalHost()!}>
        <div ref={layer.mount} data-ui="alert-dialog" data-part="root" class={dialogRoot} {...rest}>
          <button
            type="button"
            tabindex="-1"
            aria-label="Dismiss alert dialog"
            data-ui="alert-dialog-backdrop"
            data-part="backdrop"
            class={dialogBackdrop}
            onClick={(event) => {
              if (dismissable() && layer.isTopmost()) close('outside', event)
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
              if (!layer.isTopmost()) return
              if (event.key === 'Escape' && dismissable()) {
                event.stopPropagation()
                close('escape', event)
              } else {
                trapTabKey(panel!, event)
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
      </LayerPortal>
    </Show>
  )
}
