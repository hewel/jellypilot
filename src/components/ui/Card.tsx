import { cx } from '@styled-system/css';
import { splitProps } from 'solid-js';
import type { JSX } from 'solid-js';
import { Dynamic } from 'solid-js/web';

import * as styles from './Card.styles';

export type CardVariant = 'filled' | 'elevated' | 'outlined';

export interface CardProps extends JSX.HTMLAttributes<HTMLElement> {
  as?: 'div' | 'section' | 'article' | 'aside';
  variant?: CardVariant;
  padding?: 'default' | 'none';
  class?: string;
  children: JSX.Element;
}

/**
 * Control Room card surface. The only card API in the app.
 * @param variant - 'filled' (default), 'elevated', or 'outlined'
 */
export function Card(props: CardProps) {
  const [local, rest] = splitProps(props, ['as', 'variant', 'padding', 'class', 'children']);
  const variant = () => local.variant ?? 'filled';
  const padding = () => local.padding ?? 'default';

  return (
    <Dynamic
      component={local.as ?? 'div'}
      class={cx(styles.card({ variant: variant(), padding: padding() }), local.class)}
      {...rest}
    >
      <div class={styles.content}>{local.children}</div>
    </Dynamic>
  );
}

export default Card;
