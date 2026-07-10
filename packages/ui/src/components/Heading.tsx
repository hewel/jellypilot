import type { ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import { headingStyle } from './Heading.css'

export type HeadingProps = ParentProps<{
  level?: 1 | 2 | 3 | 4 | 5 | 6
  class?: string
  id?: string
}>

export function Heading(props: HeadingProps) {
  const [local, rest] = splitProps(props, ['level', 'class', 'children'])
  const level = local.level ?? 2
  const Tag = `h${level}` as 'h2'
  return (
    <Tag
      data-jp-heading=""
      data-level={String(level)}
      class={[headingStyle, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      {local.children}
    </Tag>
  )
}
