import type { ParentProps } from 'solid-js'
import {
  createContext,
  createSignal,
  For,
  onCleanup,
  Show,
  useContext,
} from 'solid-js'
import { Portal } from 'solid-js/web'
import { uiInvariant } from '../runtime/invariant'
import { toastItem, toastViewport } from './Toast.css'

export type ToastTone = 'info' | 'success' | 'warning' | 'danger'

export type ToastInput = {
  id?: string
  title: string
  description?: string
  tone?: ToastTone
  durationMs?: number
}

export type ToastRecord = Required<Pick<ToastInput, 'title'>> & {
  id: string
  description?: string
  tone: ToastTone
  durationMs: number
}

export type ToastApi = {
  toast: (input: ToastInput) => string
  dismiss: (id: string) => void
  clear: () => void
}

const ToastContext = createContext<ToastApi>()

let toastUid = 0

export type ToastProviderProps = ParentProps

export function ToastProvider(props: ToastProviderProps) {
  const [items, setItems] = createSignal<ToastRecord[]>([])
  const timers = new Map<string, ReturnType<typeof setTimeout>>()

  const dismiss = (id: string) => {
    const timer = timers.get(id)
    if (timer) clearTimeout(timer)
    timers.delete(id)
    setItems((current) => current.filter((item) => item.id !== id))
  }

  const toast = (input: ToastInput) => {
    const id = input.id ?? `toast-${++toastUid}`
    const record: ToastRecord = {
      id,
      title: input.title,
      description: input.description,
      tone: input.tone ?? 'info',
      durationMs: input.durationMs ?? 4000,
    }
    setItems((current) => {
      const without = current.filter((item) => item.id !== id)
      return [...without, record]
    })
    if (record.durationMs > 0) {
      const existing = timers.get(id)
      if (existing) clearTimeout(existing)
      timers.set(
        id,
        setTimeout(() => dismiss(id), record.durationMs),
      )
    }
    return id
  }

  const clear = () => {
    for (const timer of timers.values()) clearTimeout(timer)
    timers.clear()
    setItems([])
  }

  onCleanup(() => clear())

  const api: ToastApi = { toast, dismiss, clear }

  return (
    <ToastContext.Provider value={api}>
      {props.children}
      <Portal>
        <div data-ui="toast" data-part="viewport" class={toastViewport}>
          <For each={items()}>
            {(item) => (
              <div
                data-part="item"
                data-tone={item.tone}
                class={toastItem}
                role="status"
              >
                <strong data-part="title">{item.title}</strong>
                <Show when={item.description}>
                  <div data-part="description">{item.description}</div>
                </Show>
                <button
                  data-part="close"
                  type="button"
                  onClick={() => dismiss(item.id)}
                >
                  Dismiss
                </button>
              </div>
            )}
          </For>
        </div>
      </Portal>
    </ToastContext.Provider>
  )
}

export function useToast(): ToastApi {
  const api = useContext(ToastContext)
  uiInvariant(api, 'missing-toast', 'useToast requires ToastProvider')
  return api
}
