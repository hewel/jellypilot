import type { JSX, ParentProps } from 'solid-js'
import { createSignal, Show, splitProps } from 'solid-js'
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
  let timer: ReturnType<typeof setTimeout> | undefined

  const show = () => {
    clearTimeout(timer)
    timer = setTimeout(() => setOpen(true), local.openDelayMs ?? 250)
  }
  const hide = () => {
    clearTimeout(timer)
    setOpen(false)
  }

  return (
    <span
      data-ui="hover-card"
      data-part="root"
      data-state={open() ? 'open' : 'closed'}
      class={[hoverCardRoot, local.class].filter(Boolean).join(' ')}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocusIn={show}
      onFocusOut={hide}
      {...rest}
    >
      <span data-part="trigger">{local.children}</span>
      <Show when={open()}>
        <div data-part="content" class={hoverCardContent}>
          {local.content}
        </div>
      </Show>
    </span>
  )
}
