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
        return 'bg-tertiary-container/20 text-tertiary border-tertiary/30 shadow-[0_0_8px_rgba(79,227,177,0.12)] font-bold';
      case 'warning':
        return 'bg-warning-container/20 text-warning border-warning/30 shadow-[0_0_8px_rgba(246,199,104,0.12)] font-bold';
      case 'error':
        return 'bg-error-container/20 text-error border-error/30 shadow-[0_0_8px_rgba(255,107,122,0.12)] font-bold';
      default:
        return 'bg-surface-container-highest/30 text-on-surface-variant border-outline-variant/60 font-semibold';
    }
  };

  return (
    <span
      class={`px-3 py-1 rounded-full text-label-small border inline-flex items-center gap-1.5 shrink-0 select-none ${variantClasses()}`}
    >
      <span
        class={`w-1.5 h-1.5 rounded-full ${
          variant() === 'success'
            ? 'bg-tertiary animate-pulse'
            : variant() === 'warning'
              ? 'bg-warning animate-pulse'
              : variant() === 'error'
                ? 'bg-error animate-pulse'
                : 'bg-on-surface-variant/60'
        }`}
      />
      {props.children}
    </span>
  );
}
