import type { JSX } from 'solid-js';

interface PageHeaderProps {
  title: string;
  description?: string;
  trailing?: JSX.Element;
}

/**
 * Consistent page header with title, description, and optional trailing action.
 */
export default function PageHeader(props: PageHeaderProps) {
  return (
    <div class="flex items-center justify-between pb-4">
      <div>
        <h1 class="font-display text-on-surface text-[32px] leading-[40px] font-bold tracking-tight">
          {props.title}
        </h1>
        {props.description && (
          <p class="text-on-surface-variant mt-1 text-[16px] leading-[24px]">{props.description}</p>
        )}
      </div>
      {props.trailing}
    </div>
  );
}
