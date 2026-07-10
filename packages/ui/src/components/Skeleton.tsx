import { skeletonStyle } from './Skeleton.css'

export type SkeletonProps = {
  class?: string
  width?: string
  height?: string
}

export function Skeleton(props: SkeletonProps) {
  return (
    <div
      data-ui="skeleton"
      data-part="root"
      aria-hidden="true"
      class={[skeletonStyle, props.class].filter(Boolean).join(' ')}
      style={{
        width: props.width ?? '100%',
        height: props.height ?? '1rem',
      }}
    />
  )
}
