import { Card, Heading } from '@jellypilot/ui';
import type { JSX } from 'solid-js';

import * as styles from './ConsoleSection.css';

interface ConsoleSectionProps {
  icon: JSX.Element;
  title: string;
  children: JSX.Element;
  trailing?: JSX.Element;
}

export default function ConsoleSection(props: ConsoleSectionProps) {
  return (
    <Card class={styles.card}>
      <div class={styles.header}>
        <Heading level={2} class={styles.title}>
          {props.icon}
          {props.title}
        </Heading>
        {props.trailing}
      </div>
      {props.children}
    </Card>
  );
}
