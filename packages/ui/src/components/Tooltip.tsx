import type { JSX, ParentProps } from 'solid-js'
import { createSignal, onCleanup, Show, splitProps } from 'solid-js'
import { AnchoredLayerPortal } from '../runtime/AnchoredLayerPortal'
import { createLayerRegistration } from '../runtime/layers'
import { tooltipContent, tooltipRoot } from './Tooltip.css'

export type TooltipProps = ParentProps<{
  content: JSX.Element
  class?: string
  openDelayMs?: number
}>

export function Tooltip(props: TooltipProps) {
  const [local, rest] = splitProps(props, [
    'content',
    'class',
    'children',
    'openDelayMs',
  ])
  const [open, setOpen] = createSignal(false)
  const layer = createLayerRegistration()
  let root: HTMLSpanElement | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  onCleanup(() => clearTimeout(timer))

  const show = () => {
    clearTimeout(timer)
    timer = setTimeout(() => setOpen(true), local.openDelayMs ?? 200)
  }
  const hide = () => {
    clearTimeout(timer)
    setOpen(false)
  }

  return (
    <span
      ref={root}
      data-ui="tooltip"
      data-part="root"
      data-state={open() ? 'open' : 'closed'}
      class={[tooltipRoot, local.class].filter(Boolean).join(' ')}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocusIn={show}
      onFocusOut={hide}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && layer.isTopmost()) setOpen(false)
      }}
      {...rest}
    >
      <span data-part="trigger">{local.children}</span>
      <Show when={open() && layer.portalHost()}>
        <AnchoredLayerPortal
          mount={layer.portalHost()!}
          anchor={() => root}
          placement="top-center"
        >
          <span
            ref={layer.mount}
            data-part="content"
            role="tooltip"
            class={tooltipContent}
            style={{ position: 'static', transform: 'none' }}
          >
            {local.content}
          </span>
        </AnchoredLayerPortal>
      </Show>
    </span>
  )
}
