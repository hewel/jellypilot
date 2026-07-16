import type { JSX } from 'solid-js';

import { Card } from './Card';
import * as styles from './SectionCard.styles';

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
    <Card variant="filled">
      <div class={styles.header}>
        <h2 class={styles.title}>
          {props.icon}
          {props.title}
        </h2>
        {props.trailing}
      </div>
      {props.children}
    </Card>
  );
}
