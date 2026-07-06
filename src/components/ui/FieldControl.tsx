import { splitProps } from 'solid-js';
import type { JSX } from 'solid-js';

import * as styles from './FieldControl.css';

export type FieldControlVariant = 'filled' | 'outlined';

export interface FieldControlProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  variant?: FieldControlVariant;
  class?: string;
}

/** Control Room text input with filled (default) or outlined variant. */
export function FieldControl(props: FieldControlProps) {
  const [local, rest] = splitProps(props, ['variant', 'class']);
  const variant = () => local.variant ?? 'filled';
  return (
    <input
      class={`${styles.fieldControl({ variant: variant() })} ${local.class ?? ''}`}
      {...rest}
    />
  );
}

export interface FieldTextareaProps extends JSX.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: FieldControlVariant;
  class?: string;
}

/** Control Room textarea with filled (default) or outlined variant. */
export function FieldTextarea(props: FieldTextareaProps) {
  const [local, rest] = splitProps(props, ['variant', 'class']);
  const variant = () => local.variant ?? 'filled';
  return (
    <textarea
      class={`${styles.fieldControl({ variant: variant() })} ${local.class ?? ''}`}
      {...rest}
    />
  );
}
