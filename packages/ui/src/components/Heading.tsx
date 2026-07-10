import type { ParentProps } from 'solid-js'
import { Dynamic } from 'solid-js/web'
import { splitProps } from 'solid-js'
import { headingStyle } from './Heading.css'

export type HeadingProps = ParentProps<{
  level?: 1 | 2 | 3 | 4 | 5 | 6
  class?: string
  id?: string
}>

export function Heading(props: HeadingProps) {
  const [local, rest] = splitProps(props, ['level', 'class', 'children'])
  return (
    <Dynamic
      component={`h${local.level ?? 2}`}
      data-jp-heading=""
      data-level={String(local.level ?? 2)}
      class={[headingStyle, local.class].filter(Boolean).join(' ')}
      {...rest}
    >
      {local.children}
    </Dynamic>
  )
}
