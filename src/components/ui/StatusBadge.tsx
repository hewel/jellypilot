import * as styles from './StatusBadge.styles';

type StatusBadgeVariant = 'success' | 'warning' | 'error' | 'neutral';

interface StatusBadgeProps {
  variant?: StatusBadgeVariant;
  children: string;
}

/**
 * Control Room status badge for displaying state indicators.
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
