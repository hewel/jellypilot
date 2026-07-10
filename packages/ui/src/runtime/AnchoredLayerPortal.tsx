import type { Accessor, ParentProps } from 'solid-js'
import { createSignal, onCleanup, onMount, Show } from 'solid-js'
import { LayerPortal } from './LayerPortal'

type Placement = 'bottom-start' | 'top-center'

export function AnchoredLayerPortal(
  props: ParentProps<{
    mount: HTMLElement
    anchor: Accessor<HTMLElement | undefined>
    placement?: Placement
    matchWidth?: boolean
  }>,
) {
  const [rect, setRect] = createSignal<DOMRect | null>(null)
  const update = () => {
    const anchor = props.anchor()
    if (anchor) setRect(anchor.getBoundingClientRect())
  }

  onMount(() => {
    update()
    window.addEventListener('resize', update)
    window.addEventListener('scroll', update, true)
  })
  onCleanup(() => {
    window.removeEventListener('resize', update)
    window.removeEventListener('scroll', update, true)
  })

  const top = () => {
    const value = rect()!
    return props.placement === 'top-center' ? value.top - 6 : value.bottom + 4
  }
  const left = () => {
    const value = rect()!
    return props.placement === 'top-center' ? value.left + value.width / 2 : value.left
  }

  return (
    <Show when={rect()}>
      <LayerPortal mount={props.mount}>
        <div
          data-jp-anchored-layer=""
          style={{
            position: 'fixed',
            top: `${top()}px`,
            left: `${left()}px`,
            width: props.matchWidth ? `${rect()!.width}px` : undefined,
            transform: props.placement === 'top-center' ? 'translate(-50%, -100%)' : undefined,
            'z-index': 40,
          }}
        >
          {props.children}
        </div>
      </LayerPortal>
    </Show>
  )
}
