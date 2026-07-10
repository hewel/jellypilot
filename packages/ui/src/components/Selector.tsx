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
import { selectorContent, selectorRoot, selectorTrigger } from './Selector.css'

export type SelectorItem = {
  value: string
  label: JSX.Element
  disabled?: boolean
}

export type SelectorChangeDetails = {
  reason: 'pointer' | 'keyboard' | 'clear'
  event?: Event
}

export type SelectorProps = ParentProps<{
  value: string | null
  items: SelectorItem[]
  placeholder?: string
  disabled?: boolean
  name?: string
  class?: string
  'aria-label'?: string
  onValueChange?: (next: string | null, details: SelectorChangeDetails) => void
}>

export function Selector(props: SelectorProps) {
  const [local, rest] = splitProps(props, [
    'value',
    'items',
    'placeholder',
    'disabled',
    'name',
    'class',
    'aria-label',
    'children',
    'onValueChange',
  ])
  const [open, setOpen] = createSignal(false)
  const [activeIndex, setActiveIndex] = createSignal(-1)
  const layer = createLayerRegistration()
  let root: HTMLDivElement | undefined
  let trigger: HTMLButtonElement | undefined
  let content: HTMLDivElement | undefined
  const itemElements: HTMLButtonElement[] = []

  const keys = () => local.items.map((item) => item.value)
  uiInvariant(
    new Set(keys()).size === keys().length,
    'selector-duplicate-key',
    'Selector item values must be unique',
  )
  if (local.value !== null) {
    uiInvariant(
      keys().includes(local.value),
      'selector-unknown-value',
      `Unknown Selector value: ${local.value}`,
    )
  }

  const selected = () =>
    local.items.find((item) => item.value === local.value) ?? null

  const enabledItems = () => local.items.filter((item) => !item.disabled)
  const focusIndex = (index: number) => {
    setActiveIndex(index)
    itemElements[index]?.focus()
  }
  const openSelector = () => {
    if (local.disabled) return
    setOpen(true)
    queueMicrotask(() => {
      const selectedIndex = enabledItems().findIndex((item) => item.value === local.value)
      if (enabledItems().length > 0) focusIndex(Math.max(0, selectedIndex))
    })
  }

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

  const select = (
    value: string,
    reason: SelectorChangeDetails['reason'],
    event?: Event,
  ) => {
    const item = local.items.find((entry) => entry.value === value)
    if (!item || item.disabled || local.disabled) return
    local.onValueChange?.(value, { reason, event })
    setOpen(false)
    trigger?.focus()
  }

  return (
    <div
      ref={root}
      data-ui="selector"
      data-part="root"
      data-state={open() ? 'open' : 'closed'}
      class={[selectorRoot, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      <button
        ref={trigger}
        data-part="trigger"
        type="button"
        class={selectorTrigger}
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-label={local['aria-label']}
        disabled={local.disabled}
        onClick={() => (open() ? setOpen(false) : openSelector())}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            openSelector()
          }
        }}
      >
        {selected()?.label ?? local.placeholder ?? 'Select'}
      </button>
      <Show when={open() && layer.portalHost()}>
        <AnchoredLayerPortal mount={layer.portalHost()!} anchor={() => trigger} matchWidth>
        <div
          ref={(element) => {
            content = element
            layer.mount()
          }}
          data-part="content"
          role="listbox"
          tabindex="-1"
          class={selectorContent}
          style={{ position: 'static' }}
          onKeyDown={(event) => {
            const items = enabledItems()
            if (items.length === 0) return
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault()
              const delta = event.key === 'ArrowDown' ? 1 : -1
              focusIndex((activeIndex() + delta + items.length) % items.length)
            } else if (event.key === 'Home' || event.key === 'End') {
              event.preventDefault()
              focusIndex(event.key === 'Home' ? 0 : items.length - 1)
            } else if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault()
              const item = items[activeIndex()]
              if (item) select(item.value, 'keyboard', event)
            }
          }}
        >
          <For each={enabledItems()}>
            {(item, index) => (
              <button
                ref={(element) => (itemElements[index()] = element)}
                data-part="item"
                data-selected={item.value === local.value ? 'true' : 'false'}
                type="button"
                role="option"
                aria-selected={item.value === local.value}
                tabindex={index() === activeIndex() ? 0 : -1}
                onClick={(event) => select(item.value, 'pointer', event)}
              >
                {item.label}
              </button>
            )}
          </For>
        </div>
        </AnchoredLayerPortal>
      </Show>
      <Show when={local.name}>
        <input type="hidden" name={local.name} value={local.value ?? ''} />
      </Show>
    </div>
  )
}
