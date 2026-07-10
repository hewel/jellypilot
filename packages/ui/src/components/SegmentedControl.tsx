import type { JSX, ParentProps } from 'solid-js'
import { For, Show, splitProps } from 'solid-js'
import { uiInvariant } from '../runtime/invariant'
import { segmentedItem, segmentedRoot } from './SegmentedControl.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type SegmentedItem = {
  value: string
  label: JSX.Element
  disabled?: boolean
  icon?: JSX.Element
}

export type SegmentedChangeDetails = {
  reason: 'pointer' | 'keyboard'
  event: Event
}

export type SegmentedControlProps = ParentProps<{
  value: string
  items: SegmentedItem[]
  name?: string
  disabled?: boolean
  size?: 'sm' | 'md' | 'lg'
  layout?: 'hug' | 'fill'
  orientation?: 'horizontal' | 'vertical'
  class?: string
  'aria-label'?: string
  onValueChange?: (next: string, details: SegmentedChangeDetails) => void
}>

export function SegmentedControl(props: SegmentedControlProps) {
  const [local, rest] = splitProps(props, [
    'value',
    'items',
    'name',
    'disabled',
    'size',
    'layout',
    'orientation',
    'class',
    'aria-label',
    'children',
    'onValueChange',
  ])
  const groupId = nextId('segmented')
  const keys = () => local.items.map((item) => item.value)
  uiInvariant(
    new Set(keys()).size === keys().length,
    'segmented-duplicate-key',
    'SegmentedControl item values must be unique',
  )
  uiInvariant(
    keys().includes(local.value),
    'segmented-unknown-value',
    `Unknown SegmentedControl value: ${local.value}`,
  )

  const enabledIndexes = () =>
    local.items
      .map((item, index) => ({ item, index }))
      .filter(({ item }) => !item.disabled && !local.disabled)

  const selectIndex = (
    index: number,
    reason: SegmentedChangeDetails['reason'],
    event: Event,
  ) => {
    const entry = local.items[index]
    if (!entry || entry.disabled || local.disabled) return
    local.onValueChange?.(entry.value, { reason, event })
  }

  const move = (delta: number, event: KeyboardEvent) => {
    const enabled = enabledIndexes()
    if (enabled.length === 0) return
    const current = enabled.findIndex(({ item }) => item.value === local.value)
    const next =
      enabled[(current + delta + enabled.length) % enabled.length]!.index
    selectIndex(next, 'keyboard', event)
  }

  return (
    <div
      data-ui="segmented-control"
      data-part="root"
      data-size={local.size ?? 'md'}
      data-layout={local.layout ?? 'hug'}
      data-orientation={local.orientation ?? 'horizontal'}
      role="radiogroup"
      aria-label={local['aria-label']}
      class={[segmentedRoot, local.class].filter(Boolean).join(' ')}
      onKeyDown={(event) => {
        if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
          event.preventDefault()
          move(1, event)
        } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
          event.preventDefault()
          move(-1, event)
        } else if (event.key === 'Home') {
          event.preventDefault()
          selectIndex(enabledIndexes()[0]!.index, 'keyboard', event)
        } else if (event.key === 'End') {
          event.preventDefault()
          const last = enabledIndexes().at(-1)
          if (last) selectIndex(last.index, 'keyboard', event)
        }
      }}
      {...rest}
    >
      <For each={local.items}>
        {(item, index) => {
          const selected = () => item.value === local.value
          return (
            <button
              data-part="item"
              data-selected={selected() ? 'true' : 'false'}
              data-disabled={item.disabled || local.disabled ? 'true' : undefined}
              type="button"
              role="radio"
              aria-checked={selected()}
              disabled={item.disabled || local.disabled}
              tabIndex={selected() ? 0 : -1}
              class={segmentedItem}
              id={`${groupId}-${item.value}`}
              onClick={(event) => selectIndex(index(), 'pointer', event)}
            >
              {item.icon}
              <span data-part="label">{item.label}</span>
            </button>
          )
        }}
      </For>
      <Show when={local.name}>
        <input type="hidden" name={local.name} value={local.value} />
      </Show>
    </div>
  )
}
