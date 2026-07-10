import type { ParentProps } from 'solid-js'
import { For, Show, splitProps } from 'solid-js'
import { uiInvariant } from '../runtime/invariant'
import { fieldDescription, fieldError, fieldLabel, fieldRoot } from './Field.css'
import { sliderFill, sliderRoot, sliderThumb, sliderTrack } from './Slider.css'

let uid = 0
const nextId = (prefix: string) => `${prefix}-${++uid}`

export type SliderChangeDetails = {
  reason: 'pointer' | 'keyboard' | 'change-end'
  event: Event
}

export type SliderProps = ParentProps<{
  value: number | [number, number]
  min?: number
  max?: number
  step?: number
  name?: string | [string, string]
  label?: string
  description?: string
  error?: string
  disabled?: boolean
  orientation?: 'horizontal' | 'vertical'
  marks?: Array<{ value: number; label?: string }>
  class?: string
  id?: string
  onValueChange?: (
    next: number | [number, number],
    details: SliderChangeDetails,
  ) => void
  onValueChangeEnd?: (
    next: number | [number, number],
    details: SliderChangeDetails,
  ) => void
}>

function isRange(value: number | [number, number]): value is [number, number] {
  return Array.isArray(value)
}

export function Slider(props: SliderProps) {
  const [local, rest] = splitProps(props, [
    'value',
    'min',
    'max',
    'step',
    'name',
    'label',
    'description',
    'error',
    'disabled',
    'orientation',
    'marks',
    'class',
    'id',
    'children',
    'onValueChange',
    'onValueChangeEnd',
  ])
  const min = () => local.min ?? 0
  const max = () => local.max ?? 100
  const step = () => local.step ?? 1
  const orientation = () => local.orientation ?? 'horizontal'
  const autoId = nextId('slider')
  const rootId = () => local.id ?? `jp-${autoId}`

  uiInvariant(max() > min(), 'slider-range', 'Slider max must be greater than min')
  uiInvariant(step() > 0, 'slider-step', 'Slider step must be > 0')
  if (isRange(local.value)) {
    uiInvariant(
      local.value[0] <= local.value[1],
      'slider-order',
      'Range slider values must be ordered low to high',
    )
  }

  const clamp = (value: number) => {
    const stepped = Math.round((value - min()) / step()) * step() + min()
    return Math.min(max(), Math.max(min(), stepped))
  }

  const percent = (value: number) => ((value - min()) / (max() - min())) * 100

  const emit = (
    next: number | [number, number],
    reason: SliderChangeDetails['reason'],
    event: Event,
  ) => {
    local.onValueChange?.(next, { reason, event })
  }

  const onThumbKey = (index: 0 | 1 | 'single', event: KeyboardEvent) => {
    if (local.disabled) return
    const delta =
      event.key === 'ArrowRight' || event.key === 'ArrowUp'
        ? step()
        : event.key === 'ArrowLeft' || event.key === 'ArrowDown'
          ? -step()
          : event.key === 'Home'
            ? min() - max()
            : event.key === 'End'
              ? max() - min()
              : 0
    if (delta === 0 && event.key !== 'Home' && event.key !== 'End') return
    event.preventDefault()
    if (!isRange(local.value)) {
      emit(clamp(local.value + delta), 'keyboard', event)
      return
    }
    const next: [number, number] = [...local.value]
    if (index === 0) next[0] = Math.min(clamp(next[0] + delta), next[1])
    else next[1] = Math.max(clamp(next[1] + delta), next[0])
    emit(next, 'keyboard', event)
  }

  return (
    <div
      data-ui="slider"
      data-part="root"
      data-orientation={orientation()}
      id={rootId()}
      class={[fieldRoot, sliderRoot, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      <Show when={local.label}>
        <div class={fieldLabel} data-part="label" id={`${rootId()}-label`}>
          {local.label}
        </div>
      </Show>
      <div data-part="track" class={sliderTrack} role="presentation">
        <div
          data-part="range"
          class={sliderFill}
          style={
            isRange(local.value)
              ? {
                  left: `${percent(local.value[0])}%`,
                  width: `${percent(local.value[1]) - percent(local.value[0])}%`,
                }
              : { width: `${percent(local.value)}%` }
          }
        />
        <Show
          when={isRange(local.value)}
          fallback={
            <button
              data-part="thumb"
              type="button"
              role="slider"
              class={sliderThumb}
              style={{ left: `${percent(local.value as number)}%` }}
              aria-valuemin={min()}
              aria-valuemax={max()}
              aria-valuenow={local.value as number}
              aria-orientation={orientation()}
              disabled={local.disabled}
              onKeyDown={(event) => onThumbKey('single', event)}
              onPointerUp={(event) =>
                local.onValueChangeEnd?.(local.value, {
                  reason: 'change-end',
                  event,
                })
              }
            />
          }
        >
          <button
            data-part="thumb"
            type="button"
            role="slider"
            class={sliderThumb}
            style={{ left: `${percent((local.value as [number, number])[0])}%` }}
            aria-valuemin={min()}
            aria-valuemax={(local.value as [number, number])[1]}
            aria-valuenow={(local.value as [number, number])[0]}
            aria-orientation={orientation()}
            disabled={local.disabled}
            onKeyDown={(event) => onThumbKey(0, event)}
          />
          <button
            data-part="thumb"
            type="button"
            role="slider"
            class={sliderThumb}
            style={{ left: `${percent((local.value as [number, number])[1])}%` }}
            aria-valuemin={(local.value as [number, number])[0]}
            aria-valuemax={max()}
            aria-valuenow={(local.value as [number, number])[1]}
            aria-orientation={orientation()}
            disabled={local.disabled}
            onKeyDown={(event) => onThumbKey(1, event)}
          />
        </Show>
      </div>
      <Show when={local.marks}>
        <div data-part="marks">
          <For each={local.marks}>
            {(mark) => (
              <span data-part="mark" data-value={mark.value}>
                {mark.label ?? mark.value}
              </span>
            )}
          </For>
        </div>
      </Show>
      <Show when={typeof local.name === 'string' && !isRange(local.value)}>
        <input type="hidden" name={local.name as string} value={String(local.value)} />
      </Show>
      <Show when={Array.isArray(local.name) && isRange(local.value)}>
        <input
          type="hidden"
          name={(local.name as [string, string])[0]}
          value={String((local.value as [number, number])[0])}
        />
        <input
          type="hidden"
          name={(local.name as [string, string])[1]}
          value={String((local.value as [number, number])[1])}
        />
      </Show>
      <Show when={local.description}>
        <div class={fieldDescription} data-part="description">
          {local.description}
        </div>
      </Show>
      <Show when={local.error}>
        <div class={fieldError} data-part="error" role="alert">
          {local.error}
        </div>
      </Show>
    </div>
  )
}
