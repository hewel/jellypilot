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
  let root: HTMLDivElement | undefined

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

  const select = (
    value: string,
    reason: SelectorChangeDetails['reason'],
    event?: Event,
  ) => {
    const item = local.items.find((entry) => entry.value === value)
    if (!item || item.disabled || local.disabled) return
    local.onValueChange?.(value, { reason, event })
    setOpen(false)
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
        data-part="trigger"
        type="button"
        class={selectorTrigger}
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-label={local['aria-label']}
        disabled={local.disabled}
        onClick={() => setOpen(!open())}
      >
        {selected()?.label ?? local.placeholder ?? 'Select'}
      </button>
      <Show when={open()}>
        <div data-part="content" role="listbox" class={selectorContent}>
          <For each={local.items.filter((item) => !item.disabled)}>
            {(item) => (
              <button
                data-part="item"
                data-selected={item.value === local.value ? 'true' : 'false'}
                type="button"
                role="option"
                aria-selected={item.value === local.value}
                onClick={(event) => select(item.value, 'pointer', event)}
              >
                {item.label}
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={local.name}>
        <input type="hidden" name={local.name} value={local.value ?? ''} />
      </Show>
    </div>
  )
}
