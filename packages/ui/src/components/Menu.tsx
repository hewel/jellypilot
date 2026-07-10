import type { JSX, ParentProps } from 'solid-js'
import {
  createEffect,
  createSignal,
  For,
  onCleanup,
  Show,
  splitProps,
} from 'solid-js'
import { uiInvariant } from '../runtime/invariant'
import { menuContent, menuRoot, menuTrigger } from './Menu.css'

export type MenuItem = {
  value: string
  label: JSX.Element
  disabled?: boolean
}

export type MenuSelectDetails = {
  reason: 'pointer' | 'keyboard'
  event?: Event
}

export type MenuProps = ParentProps<{
  items: MenuItem[]
  trigger: JSX.Element
  disabled?: boolean
  class?: string
  onSelect?: (value: string, details: MenuSelectDetails) => void
}>

export function Menu(props: MenuProps) {
  const [local, rest] = splitProps(props, [
    'items',
    'trigger',
    'disabled',
    'class',
    'children',
    'onSelect',
  ])
  const [open, setOpen] = createSignal(false)
  let root: HTMLDivElement | undefined
  let trigger: HTMLButtonElement | undefined

  uiInvariant(
    new Set(local.items.map((item) => item.value)).size === local.items.length,
    'menu-duplicate-key',
    'Menu item values must be unique',
  )

  createEffect(() => {
    if (!open()) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    const onPointer = (event: MouseEvent) => {
      if (!root?.contains(event.target as Node)) setOpen(false)
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
      data-ui="menu"
      data-part="root"
      data-state={open() ? 'open' : 'closed'}
      class={[menuRoot, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      <button
        ref={trigger}
        data-part="trigger"
        type="button"
        class={menuTrigger}
        aria-haspopup="menu"
        aria-expanded={open()}
        disabled={local.disabled}
        onClick={() => setOpen(!open())}
      >
        {local.trigger}
      </button>
      <Show when={open()}>
        <div data-part="content" role="menu" class={menuContent}>
          <For each={local.items}>
            {(item) => (
              <button
                data-part="item"
                type="button"
                role="menuitem"
                disabled={item.disabled || local.disabled}
                onClick={(event) => {
                  if (item.disabled) return
                  setOpen(false)
                  trigger?.focus()
                  local.onSelect?.(item.value, { reason: 'pointer', event })
                }}
              >
                {item.label}
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  )
}
