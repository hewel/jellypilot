import { Match } from 'effect';

type StatusBadgeVariant = 'success' | 'warning' | 'error' | 'neutral';

interface StatusBadgeProps {
  variant?: StatusBadgeVariant;
  children: string;
}

const variantClassMatcher = Match.type<StatusBadgeVariant>().pipe(
  Match.withReturnType<string>(),
  Match.when(
    'success',
    () =>
      'bg-tertiary-container/20 text-tertiary border-tertiary/30 shadow-[0_0_8px_rgba(79,227,177,0.12)] font-bold',
  ),
  Match.when(
    'warning',
    () =>
      'bg-warning-container/20 text-warning border-warning/30 shadow-[0_0_8px_rgba(246,199,104,0.12)] font-bold',
  ),
  Match.when(
    'error',
    () =>
      'bg-error-container/20 text-error border-error/30 shadow-[0_0_8px_rgba(255,107,122,0.12)] font-bold',
  ),
  Match.when(
    'neutral',
    () =>
      'bg-surface-container-highest/30 text-on-surface-variant border-outline-variant/60 font-semibold',
  ),
  Match.exhaustive,
);

/**
 * Control Room status badge for displaying state indicators.
 */
export default function StatusBadge(props: StatusBadgeProps) {
  const variant = () => props.variant ?? 'neutral';

  const variantClasses = () => variantClassMatcher(variant());

  return (
    <span
      class={`text-on-surface-variant/90 inline-flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase select-none ${variantClasses()}`}
    >
      <span
        class={`h-1.5 w-1.5 rounded-full ${
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
