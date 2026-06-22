import { splitProps } from 'solid-js';
import type { JSX } from 'solid-js';

export type FieldControlVariant = 'filled' | 'outlined';

const variantClass: Record<FieldControlVariant, string> = {
  filled:
    'border-outline-variant/80 bg-surface-container-highest/30 text-on-surface placeholder-on-surface-variant/50 hover:border-secondary/40 hover:bg-surface-container-highest/40 focus:border-secondary focus:bg-surface-container-highest/60 focus:ring-secondary/15 h-14 rounded-2xl border px-4 backdrop-blur-sm transition-[background-color,border-color,box-shadow] duration-300 outline-none focus:ring-4 disabled:cursor-not-allowed disabled:opacity-50',
  outlined:
    'border-outline text-on-surface focus:border-primary focus:ring-primary/15 h-14 rounded-2xl border bg-transparent px-4 transition-[background-color,border-color,box-shadow] duration-300 outline-none focus:ring-4 disabled:cursor-not-allowed disabled:opacity-50',
};

export interface FieldControlProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  variant?: FieldControlVariant;
  class?: string;
}

/** Control Room text input with filled (default) or outlined variant. */
export function FieldControl(props: FieldControlProps) {
  const [local, rest] = splitProps(props, ['variant', 'class']);
  const variant = () => local.variant ?? 'filled';
  return <input class={`${variantClass[variant()]} ${local.class ?? ''}`} {...rest} />;
}

export interface FieldTextareaProps extends JSX.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: FieldControlVariant;
  class?: string;
}

/** Control Room textarea with filled (default) or outlined variant. */
export function FieldTextarea(props: FieldTextareaProps) {
  const [local, rest] = splitProps(props, ['variant', 'class']);
  const variant = () => local.variant ?? 'filled';
  return <textarea class={`${variantClass[variant()]} ${local.class ?? ''}`} {...rest} />;
}
