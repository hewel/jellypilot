import { Show, splitProps } from 'solid-js';
import type { JSX } from 'solid-js';
import { Dynamic } from 'solid-js/web';

import * as styles from './Card.css';

export type CardVariant = 'filled' | 'elevated' | 'outlined';

const variantClass: Record<CardVariant, string> = {
  elevated:
    'border-primary/20 bg-surface-container-low/45 rounded-[2rem] border p-6 shadow-2xl backdrop-blur-xl transition-all duration-300 hover:border-primary/35 hover:bg-surface-container-low/60',
  filled:
    'border-outline-variant/80 bg-surface/50 rounded-2xl border p-4 shadow-xl backdrop-blur-md transition-all duration-300',
  outlined:
    'border-outline-variant hover:border-outline/40 rounded-[1.75rem] border bg-transparent p-6 transition-all duration-300',
};

export interface CardProps extends JSX.HTMLAttributes<HTMLElement> {
  as?: 'div' | 'section' | 'article' | 'aside';
  variant?: CardVariant;
  surfaceTint?: boolean;
  class?: string;
  children: JSX.Element;
}

export interface CardLinkProps extends JSX.AnchorHTMLAttributes<HTMLAnchorElement> {
  variant?: CardVariant;
  surfaceTint?: boolean;
  class?: string;
  children: JSX.Element;
}

const tintOverlay = (
  <div class="bg-surface-tint/[0.03] pointer-events-none absolute inset-0 rounded-[inherit]" />
) as JSX.Element;

/**
 * Control Room card surface. The only card API in the app.
 * @param variant - 'filled' (default), 'elevated', or 'outlined'
 * @param surfaceTint - render the subtle brand tint overlay (default true)
 */
export function Card(props: CardProps) {
  const [local, rest] = splitProps(props, ['as', 'variant', 'surfaceTint', 'class', 'children']);
  const variant = () => local.variant ?? 'filled';
  const showTint = () => local.surfaceTint ?? true;

  return (
    <Dynamic
      component={local.as ?? 'div'}
      class={`${variantClass[variant()]} ${styles.cardSurface[variant()]} relative ${local.class ?? ''}`}
      {...rest}
    >
      <Show when={showTint()}>{tintOverlay}</Show>
      <div class="relative z-10">{local.children}</div>
    </Dynamic>
  );
}

/**
 * Card-styled anchor for navigational cards.
 * @param surfaceTint - render the subtle brand tint overlay (default false)
 */
export function CardLink(props: CardLinkProps) {
  const [local, rest] = splitProps(props, ['variant', 'surfaceTint', 'class', 'children']);
  const variant = () => local.variant ?? 'filled';
  const showTint = () => local.surfaceTint ?? false;

  return (
    <a
      class={`${variantClass[variant()]} ${styles.cardSurface[variant()]} relative ${local.class ?? ''}`}
      {...rest}
    >
      <Show when={showTint()}>{tintOverlay}</Show>
      <div class="relative z-10">{local.children}</div>
    </a>
  );
}

export default Card;
