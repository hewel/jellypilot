import { spinnerStyle } from './Spinner.css'

export type SpinnerProps = {
  class?: string
  label?: string
}

export function Spinner(props: SpinnerProps) {
  return (
    <span
      data-ui="spinner"
      data-part="root"
      role="status"
      aria-label={props.label ?? 'Loading'}
      class={[spinnerStyle, props.class].filter(Boolean).join(' ')}
    />
  )
}
