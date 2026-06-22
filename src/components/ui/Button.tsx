import { Show, splitProps } from 'solid-js';
import type { JSX } from 'solid-js';

export interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'tonal' | 'outlined' | 'text' | 'icon';
  size?: 'sm' | 'md' | 'lg';
  leadingIcon?: JSX.Element;
  trailingIcon?: JSX.Element;
  class?: string;
  href?: string;
}

const baseButtonClass =
  'inline-flex cursor-pointer items-center justify-center font-bold no-underline align-middle transition-[background-color,border-color,color,box-shadow,filter,transform] duration-200 select-none disabled:pointer-events-none disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary';

const sizeClass: Record<NonNullable<ButtonProps['size']>, string> = {
  lg: '[--button-py:1.2em] [--button-px:calc(var(--button-py)+(1lh-1cap)/2)] min-h-[3.25rem] rounded-[1.25rem] px-[var(--button-px)] py-[var(--button-py)] text-[16px] leading-[24px] gap-2.5',
  md: '[--button-py:0.875em] [--button-px:calc(var(--button-py)+(1lh-1cap)/2)] min-h-11 rounded-2xl px-[var(--button-px)] py-[var(--button-py)] text-[14px] leading-[20px] gap-2',
  sm: '[--button-py:0.5em] [--button-px:calc(var(--button-py)+(1lh-1cap)/2)] min-h-10 rounded-xl px-[var(--button-px)] py-[var(--button-py)] text-[12px] leading-[16px] gap-1.5',
};

const iconSizeClass: Record<NonNullable<ButtonProps['size']>, string> = {
  lg: 'h-[3.25rem] min-h-[3.25rem] w-[3.25rem] min-w-[3.25rem] rounded-[1.25rem] p-0',
  md: 'h-11 min-h-11 w-11 min-w-11 rounded-2xl p-0',
  sm: 'h-10 min-h-10 w-10 min-w-10 rounded-xl p-0',
};

const variantClass: Record<NonNullable<ButtonProps['variant']>, string> = {
  primary:
    'bg-gradient-to-r from-primary to-primary-gradient-end text-on-primary shadow-lg shadow-primary/20 hover:-translate-y-0.5 hover:brightness-110 hover:shadow-primary/45 active:translate-y-0 active:scale-[0.96]',
  secondary:
    'border border-outline-variant bg-gradient-to-r from-secondary-container to-secondary-gradient-end text-on-secondary-container shadow-md hover:border-outline hover:-translate-y-0.5 active:translate-y-0 active:scale-[0.96]',
  tonal:
    'border border-outline-variant bg-gradient-to-r from-secondary-container to-secondary-gradient-end text-on-secondary-container shadow-md hover:border-outline hover:-translate-y-0.5 active:translate-y-0 active:scale-[0.96]',
  outlined:
    'border border-outline bg-transparent text-on-surface hover:border-primary hover:bg-primary/5 active:scale-[0.96]',
  text: 'bg-transparent text-secondary hover:bg-secondary/10 active:scale-[0.96]',
  icon: 'bg-transparent text-on-surface-variant hover:bg-primary/10 hover:text-on-surface active:scale-[0.96]',
};

const iconLeadingClass =
  'inline-flex h-[1lh] w-[1lh] shrink-0 items-center justify-center ml-[calc(var(--button-py)-var(--button-px))]';
const iconTrailingClass =
  'inline-flex h-[1lh] w-[1lh] shrink-0 items-center justify-center mr-[calc(var(--button-py)-var(--button-px))]';

/**
 * Control Room Button component styled with Tailwind utilities.
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
    const classes = [baseButtonClass];
    if (variant() === 'icon') {
      classes.push(variantClass.icon, iconSizeClass[size()]);
    } else {
      classes.push(variantClass[variant()], sizeClass[size()]);
    }
    if (local.class) {
      classes.push(local.class);
    }
    return classes.join(' ');
  };

  return (
    <Show
      when={local.href}
      fallback={
        <button class={buttonClass()} {...rest}>
          <Show when={local.leadingIcon}>
            <span class={iconLeadingClass}>{local.leadingIcon}</span>
          </Show>

          {local.children}

          <Show when={local.trailingIcon}>
            <span class={iconTrailingClass}>{local.trailingIcon}</span>
          </Show>
        </button>
      }
    >
      <a href={local.href} class={buttonClass()} {...anchorRest()}>
        <Show when={local.leadingIcon}>
          <span class={iconLeadingClass}>{local.leadingIcon}</span>
        </Show>

        {local.children}

        <Show when={local.trailingIcon}>
          <span class={iconTrailingClass}>{local.trailingIcon}</span>
        </Show>
      </a>
    </Show>
  );
}
