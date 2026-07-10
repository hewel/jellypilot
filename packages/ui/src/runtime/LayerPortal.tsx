import type { ParentProps } from 'solid-js'
import { Portal } from 'solid-js/web'
import { useTheme } from '../theme/context'

export function LayerPortal(props: ParentProps<{ mount: HTMLElement }>) {
  const theme = useTheme()
  return (
    <Portal mount={props.mount}>
      <div
        data-jp-layer-portal=""
        data-theme={theme.resolved}
        data-theme-id={theme.descriptor.id}
        style={{ display: 'contents' }}
      >
        {props.children}
      </div>
    </Portal>
  )
}
