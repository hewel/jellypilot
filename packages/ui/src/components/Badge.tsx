import type { ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import { badgeStyle } from './Badge.css'

export type BadgeProps = ParentProps<{
  class?: string
  tone?: 'neutral' | 'success' | 'warning' | 'danger'
}>

export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, ['class', 'children', 'tone'])
  return (
    <span
      data-ui="badge"
      data-part="root"
      data-tone={local.tone ?? 'neutral'}
      class={[badgeStyle, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      {local.children}
    </span>
  )
}
