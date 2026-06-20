import type { JSX } from 'solid-js';

interface InfoCardProps {
  label: string;
  children: JSX.Element;
}

/**
 * Small info card for displaying labeled values (e.g., status, server name).
 */
export default function InfoCard(props: InfoCardProps) {
  return (
    <div class="bg-surface-container-high/30 border-outline-variant/60 relative overflow-hidden rounded-2xl border p-4 shadow-inner backdrop-blur-sm">
      <span class="text-on-surface-variant/90 mb-1 block text-[11px] leading-[16px] font-bold tracking-[0.08em] tracking-wider uppercase">
        {props.label}
      </span>
      {props.children}
    </div>
  );
}
