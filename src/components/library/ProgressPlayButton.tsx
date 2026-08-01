import { cx } from '@styled-system/css';
import type { JSX } from 'solid-js';
import { Show } from 'solid-js';

import { Button } from '../ui';
import * as styles from './ProgressPlayButton.styles';

/**
 * Hero and episode-row play action. Without progress it renders as a standard
 * pill; with progress it shows a watched-percent fill behind the label,
 * optionally with a "42 min remaining" second line. The fill is decorative —
 * the remaining time is conveyed by the visible second line.
 */
export function ProgressPlayButton(props: {
  label: string;
  variant?: 'primary' | 'outlined';
  percent?: number | null;
  remainingLabel?: string | null;
  disabled?: boolean;
  onClick?: () => void;
  leadingIcon?: JSX.Element;
  class?: string;
  'aria-label'?: string;
}): JSX.Element {
  const variant = () => props.variant ?? 'primary';
  const percent = () => {
    const value = props.percent;
    return value != null && value > 0 && value < 100 ? value : null;
  };
  // Name-from-content concatenates the two lines without whitespace, so give
  // assistive tech a natural single string when a remaining line is shown.
  const accessibleLabel = () => {
    const explicit = props['aria-label'];
    if (explicit) {
      return explicit;
    }
    const remaining = props.remainingLabel;
    return remaining ? `${props.label} ${remaining}` : undefined;
  };

  return (
    <Button
      type="button"
      variant={variant()}
      class={cx(
        styles.root({
          glow: variant() === 'primary',
          progress:
            percent() !== null ? (variant() === 'primary' ? 'primary' : 'outlined') : 'none',
        }),
        props.class,
      )}
      style={percent() !== null ? { '--play-progress': `${percent()}%` } : undefined}
      disabled={props.disabled}
      onClick={props.onClick}
      leadingIcon={props.leadingIcon}
      aria-label={accessibleLabel()}
    >
      <span class={styles.text}>
        <span>{props.label}</span>
        <Show when={props.remainingLabel}>
          {(remainingLabel) => <span class={styles.remaining}>{remainingLabel()}</span>}
        </Show>
      </span>
    </Button>
  );
}
