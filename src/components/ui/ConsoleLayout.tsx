import { splitProps } from 'solid-js';
import type { JSX } from 'solid-js';

export interface ConsoleShellProps extends JSX.HTMLAttributes<HTMLDivElement> {
  class?: string;
  children: JSX.Element;
}

/** Authenticated app outer shell: full-height flex column with responsive padding. */
export function ConsoleShell(props: ConsoleShellProps) {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <div
      class={`text-on-surface flex min-h-dvh flex-col justify-between p-1 sm:p-2 lg:p-4 ${local.class ?? ''}`}
      {...rest}
    >
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
    <div
      class={`mx-auto w-full animate-[fadeIn_0.3s_cubic-bezier(0.16,1,0.3,1)_forwards] space-y-6 ${local.class ?? ''}`}
      {...rest}
    >
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
    <div
      class={`grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1.3fr)_minmax(330px,0.7fr)] ${local.class ?? ''}`}
      {...rest}
    >
      {local.children}
    </div>
  );
}
