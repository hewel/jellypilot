import type { JSX } from 'solid-js';

import * as styles from './PageHeader.styles';

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
    <div class={styles.root}>
      <div>
        <h1 class={styles.title}>{props.title}</h1>
        {props.description && <p class={styles.description}>{props.description}</p>}
      </div>
      {props.trailing}
    </div>
  );
}
