import type { JSX, ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import { textStyle } from './Text.css'

export type TextProps = ParentProps<{
  as?: 'p' | 'span' | 'div'
  size?: 'sm' | 'md' | 'lg'
  class?: string
  id?: string
  style?: JSX.CSSProperties | string
}>

export function Text(props: TextProps) {
  const [local, rest] = splitProps(props, ['as', 'size', 'class', 'children'])
  const Tag = (local.as ?? 'p') as 'p'
  return (
    <Tag
      data-jp-text=""
      data-size={local.size ?? 'md'}
      class={[textStyle, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      {local.children}
    </Tag>
  )
}
