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
import { AnchoredLayerPortal } from '../runtime/AnchoredLayerPortal'
import { createLayerRegistration } from '../runtime/layers'
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
  const [activeIndex, setActiveIndex] = createSignal(-1)
  const layer = createLayerRegistration()
  let root: HTMLDivElement | undefined
  let trigger: HTMLButtonElement | undefined
  let content: HTMLDivElement | undefined
  const itemElements: HTMLButtonElement[] = []

  const enabledIndexes = () =>
    local.items.flatMap((item, index) => (item.disabled || local.disabled ? [] : [index]))
  const focusIndex = (index: number) => {
    setActiveIndex(index)
    itemElements[index]?.focus()
  }
  const openMenu = () => {
    if (local.disabled) return
    setOpen(true)
    queueMicrotask(() => {
      const first = enabledIndexes()[0]
      if (first !== undefined) focusIndex(first)
    })
  }

  uiInvariant(
    new Set(local.items.map((item) => item.value)).size === local.items.length,
    'menu-duplicate-key',
    'Menu item values must be unique',
  )

  createEffect(() => {
    if (!open()) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && layer.isTopmost()) {
        setOpen(false)
        trigger?.focus()
      }
    }
    const onPointer = (event: MouseEvent) => {
      if (
        layer.isTopmost() &&
        !root?.contains(event.target as Node) &&
        !content?.contains(event.target as Node)
      ) {
        setOpen(false)
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
        onClick={() => (open() ? setOpen(false) : openMenu())}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            openMenu()
          }
        }}
      >
        {local.trigger}
      </button>
      <Show when={open() && layer.portalHost()}>
        <AnchoredLayerPortal mount={layer.portalHost()!} anchor={() => trigger}>
        <div
          ref={(element) => {
            content = element
            layer.mount()
          }}
          data-part="content"
          role="menu"
          tabindex="-1"
          class={menuContent}
          style={{ position: 'static' }}
          onKeyDown={(event) => {
            const indexes = enabledIndexes()
            if (indexes.length === 0) return
            const position = Math.max(0, indexes.indexOf(activeIndex()))
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault()
              const delta = event.key === 'ArrowDown' ? 1 : -1
              focusIndex(indexes[(position + delta + indexes.length) % indexes.length]!)
            } else if (event.key === 'Home' || event.key === 'End') {
              event.preventDefault()
              focusIndex(event.key === 'Home' ? indexes[0]! : indexes.at(-1)!)
            } else if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault()
              const item = local.items[activeIndex()]
              if (!item || item.disabled) return
              setOpen(false)
              trigger?.focus()
              local.onSelect?.(item.value, { reason: 'keyboard', event })
            }
          }}
        >
          <For each={local.items}>
            {(item, index) => (
              <button
                ref={(element) => (itemElements[index()] = element)}
                data-part="item"
                type="button"
                role="menuitem"
                disabled={item.disabled || local.disabled}
                tabindex={index() === activeIndex() ? 0 : -1}
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
        </AnchoredLayerPortal>
      </Show>
    </div>
  )
}
