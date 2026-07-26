import type { StatusBadgeVariant } from './StatusBadge.styles';
import * as styles from './StatusBadge.styles';

interface StatusBadgeProps {
  variant?: StatusBadgeVariant;
  children: string;
}

/**
 * Control Room status badge for displaying state indicators.
 * Text always accompanies the LED so color is never the only status signal.
 */
export default function StatusBadge(props: StatusBadgeProps) {
  const variant = () => props.variant ?? 'neutral';

  return (
    <span class={styles.statusBadge({ variant: variant() })}>
      <span class={styles.statusDot({ variant: variant() })} />
      {props.children}
    </span>
  );
}
