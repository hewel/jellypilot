import { progressRoot, progressFill } from './ProgressBar.css'

export type ProgressBarProps = {
  value: number
  max?: number
  class?: string
  label?: string
}

export function ProgressBar(props: ProgressBarProps) {
  const max = () => props.max ?? 100
  const pct = () => Math.max(0, Math.min(100, (props.value / max()) * 100))
  return (
    <div
      data-ui="progress-bar"
      data-part="root"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={max()}
      aria-valuenow={props.value}
      aria-label={props.label}
      class={[progressRoot, props.class].filter(Boolean).join(' ')}
    >
      <div
        data-part="fill"
        class={progressFill}
        style={{ width: `${pct()}%` }}
      />
    </div>
  )
}
