import type { ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import { cardStyle } from './Card.css'

export type CardProps = ParentProps<{
  class?: string
  variant?: 'filled' | 'outlined'
}>

export function Card(props: CardProps) {
  const [local, rest] = splitProps(props, ['class', 'children', 'variant'])
  return (
    <div
      data-ui="card"
      data-part="root"
      data-variant={local.variant ?? 'filled'}
      class={[cardStyle, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      {local.children}
    </div>
  )
}
