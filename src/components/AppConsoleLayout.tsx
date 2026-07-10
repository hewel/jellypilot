import { splitProps } from 'solid-js';
import type { JSX } from 'solid-js';

import * as styles from './AppConsoleLayout.css';

export interface ConsoleShellProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string;
  children: JSX.Element;
}

/** Authenticated app outer shell: full-height flex column with responsive padding. */
export function ConsoleShell(props: ConsoleShellProps) {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={`${styles.consoleShell} ${local.class ?? ''}`} {...rest}>
      {local.children}
    </div>
  );
}

export interface ConsoleContainerProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string;
  children: JSX.Element;
}

/** Centered content container with entrance animation. */
export function ConsoleContainer(props: ConsoleContainerProps) {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={`${styles.consoleContainer} ${local.class ?? ''}`} {...rest}>
      {local.children}
    </div>
  );
}

export interface ConsoleGridProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string;
  children: JSX.Element;
}

/** Two-column responsive grid for console layouts. */
export function ConsoleGrid(props: ConsoleGridProps) {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div class={`${styles.consoleGrid} ${local.class ?? ''}`} {...rest}>
      {local.children}
    </div>
  );
}
