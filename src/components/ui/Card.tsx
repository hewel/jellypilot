import { Show, splitProps } from 'solid-js';
import type { JSX } from 'solid-js';
import { Dynamic } from 'solid-js/web';

import * as styles from './Card.css';

export type CardVariant = 'filled' | 'elevated' | 'outlined';

export interface CardProps extends JSX.HTMLAttributes<HTMLElement> {
  as?: 'div' | 'section' | 'article' | 'aside';
  variant?: CardVariant;
  padding?: 'default' | 'none';
  surfaceTint?: boolean;
  class?: string;
  children: JSX.Element;
}

const tintOverlay = (<div class={styles.tintOverlay} />) as JSX.Element;

/**
 * Control Room card surface. The only card API in the app.
 * @param variant - 'filled' (default), 'elevated', or 'outlined'
 * @param surfaceTint - render the subtle brand tint overlay (default true)
 */
export function Card(props: CardProps) {
  const [local, rest] = splitProps(props, [
    'as',
    'variant',
    'padding',
    'surfaceTint',
    'class',
    'children',
  ]);
  const variant = () => local.variant ?? 'filled';
  const padding = () => local.padding ?? 'default';
  const showTint = () => local.surfaceTint ?? true;

  return (
    <Dynamic
      component={local.as ?? 'div'}
      class={`${styles.card({ variant: variant(), padding: padding() })} ${styles.cardSurface[variant()]} ${local.class ?? ''}`}
      {...rest}
    >
      <Show when={showTint()}>{tintOverlay}</Show>
      <div class={styles.content}>{local.children}</div>
    </Dynamic>
  );
}

export default Card;
