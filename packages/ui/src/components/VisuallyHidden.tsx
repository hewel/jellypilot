import type { JSX, ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import { visuallyHiddenStyle } from './VisuallyHidden.css'

export type VisuallyHiddenProps = ParentProps<
  JSX.HTMLAttributes<HTMLSpanElement>
>

export function VisuallyHidden(props: VisuallyHiddenProps) {
  const [local, rest] = splitProps(props, ['class', 'children'])
  return (
    <span
      data-jp-visually-hidden=""
      class={[visuallyHiddenStyle, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      {local.children}
    </span>
  )
}
