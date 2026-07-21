import { cx } from '@styled-system/css';
import { Show, splitProps } from 'solid-js';
import type { JSX } from 'solid-js';

import * as styles from './Button.styles';

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'tonal' | 'outlined' | 'text' | 'icon';
  /** `row` is only valid with `variant="icon"` (full-width rail row); other variants fall back to `md`. */
  size?: 'sm' | 'md' | 'lg' | 'row';
  leadingIcon?: JSX.Element;
  trailingIcon?: JSX.Element;
  class?: string;
  href?: string;
}

/**
 * Control Room Button component styled with Panda recipes.
 * Supports design system variants (primary, secondary, tonal, outlined, text, icon), sizes,
 * and automatically renders as an `<a>` element if an `href` prop is supplied.
 */
export default function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, [
    'variant',
    'size',
    'leadingIcon',
    'trailingIcon',
    'class',
    'children',
    'href',
  ]);

  const variant = () => local.variant ?? 'primary';
  const size = () => local.size ?? 'md';
  const anchorRest = () => rest as unknown as JSX.AnchorHTMLAttributes<HTMLAnchorElement>;

  const buttonClass = () => {
    const currentVariant = variant();
    const currentSize = size();
    if (currentVariant === 'icon') {
      return cx(styles.iconButton({ size: currentSize }), local.class);
    }
    const pillSize = currentSize === 'row' ? 'md' : currentSize;
    return cx(styles.button({ variant: currentVariant, size: pillSize }), local.class);
  };

  return (
    <Show
      when={local.href}
      fallback={
        <button class={buttonClass()} {...rest}>
          <Show when={local.leadingIcon}>
            <span class={styles.buttonIcon}>{local.leadingIcon}</span>
          </Show>

          {local.children}

          <Show when={local.trailingIcon}>
            <span class={styles.buttonIcon}>{local.trailingIcon}</span>
          </Show>
        </button>
      }
    >
      <a href={local.href} class={buttonClass()} {...anchorRest()}>
        <Show when={local.leadingIcon}>
          <span class={styles.buttonIcon}>{local.leadingIcon}</span>
        </Show>

        {local.children}

        <Show when={local.trailingIcon}>
          <span class={styles.buttonIcon}>{local.trailingIcon}</span>
        </Show>
      </a>
    </Show>
  );
}
