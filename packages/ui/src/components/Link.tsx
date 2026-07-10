import type { ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import {
  useLinkAdapter,
  type LinkRenderProps,
} from '../runtime/link-adapter'

export type LinkProps = ParentProps<{
  href: string
  class?: string
  target?: string
  rel?: string
  'aria-label'?: string
}>

/** Navigation-only control. Default native anchor; adapter from UIRoot. */
export function Link(props: LinkProps) {
  const [local, rest] = splitProps(props, [
    'href',
    'class',
    'children',
    'target',
    'rel',
  ])
  const adapter = useLinkAdapter()
  if (adapter) {
    const renderProps: LinkRenderProps = {
      href: local.href,
      class: local.class,
      target: local.target,
      rel: local.rel,
      children: local.children,
    }
    return adapter(renderProps)
  }
  return (
    <a
      data-jp-link=""
      href={local.href}
      class={local.class}
      target={local.target}
      rel={local.rel}
      {...rest}
    >
      {local.children}
    </a>
  )
}
