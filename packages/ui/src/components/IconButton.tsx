import type { JSX, ParentProps } from 'solid-js'
import { splitProps } from 'solid-js'
import { uiInvariant } from '../runtime/invariant'
import {
  buttonBase,
  buttonGhost,
  buttonOutline,
  buttonPrimary,
  buttonSecondary,
  iconButton,
} from './ActionControl.css'
import type { ButtonVariant } from './Button'

export type IconButtonProps = ParentProps<{
  'aria-label': string
  variant?: ButtonVariant
  disabled?: boolean
  loading?: boolean
  type?: 'button' | 'submit' | 'reset'
  class?: string
  onClick?: JSX.EventHandlerUnion<HTMLButtonElement, MouseEvent>
}> &
  JSX.HTMLAttributes<HTMLButtonElement>

const variantClass: Record<ButtonVariant, string> = {
  primary: buttonPrimary,
  secondary: buttonSecondary,
  outline: buttonOutline,
  ghost: buttonGhost,
}

export function IconButton(props: IconButtonProps) {
  const [local, rest] = splitProps(props, [
    'variant',
    'disabled',
    'loading',
    'class',
    'children',
    'type',
    'aria-label',
  ])
  uiInvariant(
    local['aria-label']?.trim().length > 0,
    'iconbutton-name',
    'IconButton requires a non-empty aria-label',
  )
  const variant = () => local.variant ?? 'ghost'
  return (
    <button
      data-ui="icon-button"
      data-part="root"
      data-variant={variant()}
      data-loading={local.loading ? 'true' : undefined}
      type={local.type ?? 'button'}
      aria-label={local['aria-label']}
      disabled={local.disabled || local.loading}
      class={[
        buttonBase,
        iconButton,
        variantClass[variant()],
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
