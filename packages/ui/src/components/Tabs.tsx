import type { JSX, ParentProps } from 'solid-js'
import { For, Show, splitProps } from 'solid-js'
import { uiInvariant } from '../runtime/invariant'
import { tabList, tabPanel, tabRoot, tabTrigger } from './Tabs.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type TabItem = {
  value: string
  label: JSX.Element
  content: JSX.Element
  disabled?: boolean
}

export type TabsChangeDetails = {
  reason: 'pointer' | 'keyboard'
  event: Event
}

export type TabsProps = ParentProps<{
  value: string
  items: TabItem[]
  orientation?: 'horizontal' | 'vertical'
  class?: string
  onValueChange?: (next: string, details: TabsChangeDetails) => void
}>

export function Tabs(props: TabsProps) {
  const [local, rest] = splitProps(props, [
    'value',
    'items',
    'orientation',
    'class',
    'children',
    'onValueChange',
  ])
  const baseId = nextId('tabs')
  const keys = () => local.items.map((item) => item.value)
  uiInvariant(
    new Set(keys()).size === keys().length,
    'tabs-duplicate-key',
    'Tabs item values must be unique',
  )
  uiInvariant(
    keys().includes(local.value),
    'tabs-unknown-value',
    `Unknown Tabs value: ${local.value}`,
  )

  const enabled = () =>
    local.items
      .map((item, index) => ({ item, index }))
      .filter(({ item }) => !item.disabled)

  const select = (
    value: string,
    reason: TabsChangeDetails['reason'],
    event: Event,
  ) => {
    const item = local.items.find((entry) => entry.value === value)
    if (!item || item.disabled) return
    local.onValueChange?.(value, { reason, event })
  }

  const move = (delta: number, event: KeyboardEvent) => {
    const list = enabled()
    if (list.length === 0) return
    const current = list.findIndex(({ item }) => item.value === local.value)
    const next = list[(current + delta + list.length) % list.length]!.item.value
    select(next, 'keyboard', event)
  }

  const selected = () => local.items.find((item) => item.value === local.value)

  return (
    <div
      data-ui="tabs"
      data-part="root"
      data-orientation={local.orientation ?? 'horizontal'}
      class={[tabRoot, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      <div
        data-part="list"
        role="tablist"
        aria-orientation={local.orientation ?? 'horizontal'}
        class={tabList}
        onKeyDown={(event) => {
          if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
            event.preventDefault()
            move(1, event)
          } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
            event.preventDefault()
            move(-1, event)
          } else if (event.key === 'Home') {
            event.preventDefault()
            select(enabled()[0]!.item.value, 'keyboard', event)
          } else if (event.key === 'End') {
            event.preventDefault()
            select(enabled().at(-1)!.item.value, 'keyboard', event)
          }
        }}
      >
        <For each={local.items}>
          {(item) => {
            const isSelected = () => item.value === local.value
            return (
              <button
                data-part="trigger"
                data-selected={isSelected() ? 'true' : 'false'}
                type="button"
                role="tab"
                id={`${baseId}-tab-${item.value}`}
                aria-controls={`${baseId}-panel-${item.value}`}
                aria-selected={isSelected()}
                disabled={item.disabled}
                tabIndex={isSelected() ? 0 : -1}
                class={tabTrigger}
                onClick={(event) => select(item.value, 'pointer', event)}
              >
                {item.label}
              </button>
            )
          }}
        </For>
      </div>
      <Show when={selected()}>
        {(item) => (
          <div
            data-part="content"
            role="tabpanel"
            id={`${baseId}-panel-${item().value}`}
            aria-labelledby={`${baseId}-tab-${item().value}`}
            class={tabPanel}
          >
            {item().content}
          </div>
        )}
      </Show>
    </div>
  )
}
