import type { JSX } from 'solid-js';

import * as styles from './InfoCard.css';

interface InfoCardProps {
  label: string;
  children: JSX.Element;
}

/**
 * Small info card for displaying labeled values (e.g., status, server name).
 */
export default function InfoCard(props: InfoCardProps) {
  return (
    <div class={styles.root}>
      <span class={styles.label}>{props.label}</span>
      {props.children}
    </div>
  );
}
