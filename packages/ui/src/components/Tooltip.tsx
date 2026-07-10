import type { JSX, ParentProps } from 'solid-js'
import { createSignal, Show, splitProps } from 'solid-js'
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
  let timer: ReturnType<typeof setTimeout> | undefined

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
      data-ui="tooltip"
      data-part="root"
      data-state={open() ? 'open' : 'closed'}
      class={[tooltipRoot, local.class].filter(Boolean).join(' ')}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocusIn={show}
      onFocusOut={hide}
      {...rest}
    >
      <span data-part="trigger">{local.children}</span>
      <Show when={open()}>
        <span data-part="content" role="tooltip" class={tooltipContent}>
          {local.content}
        </span>
      </Show>
    </span>
  )
}
