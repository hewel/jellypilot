import type { JSX } from 'solid-js';

interface SectionCardProps {
  icon: JSX.Element;
  title: string;
  children: JSX.Element;
  trailing?: JSX.Element;
}

/**
 * Control Room section card with icon + title header.
 */
export default function SectionCard(props: SectionCardProps) {
  return (
    <div class="card-filled relative overflow-hidden">
      <div class="absolute inset-0 bg-surface-tint/[0.03] pointer-events-none" />
      <div class="relative z-10">
        <div class="flex items-center justify-between mb-6">
          <h2 class="text-title-medium text-primary flex items-center gap-3">
            {props.icon}
            {props.title}
          </h2>
          {props.trailing}
        </div>
        {props.children}
      </div>
    </div>
  );
}
