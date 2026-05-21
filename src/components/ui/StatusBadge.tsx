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

  const variantClasses = () => {
    switch (variant()) {
      case 'success':
        return 'bg-tertiary-container text-on-tertiary-container border-tertiary/20';
      case 'warning':
        return 'bg-warning-container text-on-warning-container border-warning/20';
      case 'error':
        return 'bg-error-container text-on-error-container border-error/20';
      default:
        return 'bg-surface-container-highest text-on-surface-variant border-outline-variant';
    }
  };

  return (
    <span
      class={`px-3 py-1 rounded-full text-label-small border ${variantClasses()}`}
    >
      {props.children}
    </span>
  );
}
