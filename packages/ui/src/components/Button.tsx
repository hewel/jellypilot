import type { JSX, ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import {
  buttonBase,
  buttonGhost,
  buttonLg,
  buttonMd,
  buttonOutline,
  buttonPrimary,
  buttonSecondary,
  buttonSm,
} from './Button.css'

export type ButtonVariant = 'primary' | 'secondary' | 'outline' | 'ghost'
export type ButtonSize = 'sm' | 'md' | 'lg'

export type ButtonProps = ParentProps<{
  variant?: ButtonVariant
  size?: ButtonSize
  disabled?: boolean
  loading?: boolean
  type?: 'button' | 'submit' | 'reset'
  class?: string
  id?: string
  'aria-label'?: string
  onClick?: JSX.EventHandlerUnion<HTMLButtonElement, MouseEvent>
}>

const variantClass: Record<ButtonVariant, string> = {
  primary: buttonPrimary,
  secondary: buttonSecondary,
  outline: buttonOutline,
  ghost: buttonGhost,
}

const sizeClass: Record<ButtonSize, string> = {
  sm: buttonSm,
  md: buttonMd,
  lg: buttonLg,
}

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, [
    'variant',
    'size',
    'disabled',
    'loading',
    'class',
    'children',
    'type',
  ])
  const variant = () => local.variant ?? 'primary'
  const size = () => local.size ?? 'md'
  return (
    <button
      data-ui="button"
      data-part="root"
      data-variant={variant()}
      data-size={size()}
      data-loading={local.loading ? 'true' : undefined}
      type={local.type ?? 'button'}
      disabled={local.disabled || local.loading}
      class={[
        buttonBase,
        variantClass[variant()],
        sizeClass[size()],
        local.class,
      ]
        .filter(Boolean)
        .join(' ')}
      {...rest}
    >
      {local.children}
    </button>
  )
}
