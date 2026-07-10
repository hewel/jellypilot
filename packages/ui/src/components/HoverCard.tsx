import type { JSX, ParentProps } from 'solid-js'
import { createSignal, onCleanup, Show, splitProps } from 'solid-js'
import { AnchoredLayerPortal } from '../runtime/AnchoredLayerPortal'
import { createLayerRegistration } from '../runtime/layers'
import { hoverCardContent, hoverCardRoot } from './HoverCard.css'

export type HoverCardProps = ParentProps<{
  content: JSX.Element
  class?: string
  openDelayMs?: number
}> &
  JSX.HTMLAttributes<HTMLSpanElement>

export function HoverCard(props: HoverCardProps) {
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
    timer = setTimeout(() => setOpen(true), local.openDelayMs ?? 250)
  }
  const hide = () => {
    clearTimeout(timer)
    timer = setTimeout(() => setOpen(false), 100)
  }

  return (
    <span
      ref={root}
      data-ui="hover-card"
      data-part="root"
      data-state={open() ? 'open' : 'closed'}
      class={[hoverCardRoot, local.class].filter(Boolean).join(' ')}
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
        <AnchoredLayerPortal mount={layer.portalHost()!} anchor={() => root}>
          <div
            ref={layer.mount}
            data-part="content"
            class={hoverCardContent}
            style={{ position: 'static' }}
            onMouseEnter={() => clearTimeout(timer)}
            onMouseLeave={hide}
          >
            {local.content}
          </div>
        </AnchoredLayerPortal>
      </Show>
    </span>
  )
}
