import type { JSX } from 'solid-js';

import { Card } from './Card';

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
    <Card variant="filled" class="relative">
      <div class="mb-6 flex items-center justify-between">
        <h2 class="text-primary flex items-center gap-3 text-[16px] leading-[24px] font-semibold">
          {props.icon}
          {props.title}
        </h2>
        {props.trailing}
      </div>
      {props.children}
    </Card>
  );
}
