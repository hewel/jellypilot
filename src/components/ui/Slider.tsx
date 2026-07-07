import { createMemo, createSignal } from 'solid-js';

import * as styles from './Slider.css';

export interface SliderProps {
  label: string;
  min: number;
  max: number;
  value: number;
  disabled?: boolean;
  class?: string;
  controlClass?: string;
  trackClass?: string;
  rangeClass?: string;
  thumbClass?: string;
  onValueChange: (value: number) => void;
  onValueCommit: (value: number) => void;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export default function Slider(props: SliderProps) {
  const [focusVisible, setFocusVisible] = createSignal(false);
  const disabled = () => props.disabled === true;
  const safeMax = () => Math.max(props.min, props.max);
  const currentValue = () => clamp(props.value, props.min, safeMax());
  const percent = createMemo(() => {
    const range = safeMax() - props.min;

    if (range <= 0) {
      return 0;
    }

    return ((currentValue() - props.min) / range) * 100;
  });

  return (
    <div class={props.class} data-disabled={disabled() ? '' : undefined}>
      <div class={props.controlClass}>
        <div class={props.trackClass} aria-hidden="true">
          <div class={props.rangeClass} style={{ width: `${percent()}%` }} />
        </div>
        <span class={styles.thumbPosition} style={{ left: `${percent()}%` }} aria-hidden="true">
          <span
            class={props.thumbClass}
            data-focus-visible={focusVisible() ? '' : undefined}
            data-disabled={disabled() ? '' : undefined}
          />
        </span>
        <input
          type="range"
          class={styles.input}
          aria-label={props.label}
          aria-valuemin={props.min}
          aria-valuemax={safeMax()}
          aria-valuenow={currentValue()}
          min={props.min}
          max={safeMax()}
          value={currentValue()}
          disabled={disabled()}
          onInput={(event) => props.onValueChange(event.currentTarget.valueAsNumber)}
          onChange={(event) => props.onValueCommit(event.currentTarget.valueAsNumber)}
          onFocus={(event) => setFocusVisible(event.currentTarget.matches(':focus-visible'))}
          onBlur={() => setFocusVisible(false)}
        />
      </div>
    </div>
  );
}
