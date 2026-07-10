import type { ParentProps } from 'solid-js'
import { createMemo } from 'solid-js'
import { ThemeContext, useTheme } from '../theme/context'
import type { ThemeDescriptor, ThemeMode } from '../theme/types'

export type ThemeProps = ParentProps<{
  descriptor?: ThemeDescriptor
  mode?: ThemeMode
}>

/** Nested theme scope; inherits preference resolution from UIRoot. */
export function Theme(props: ThemeProps) {
  const parent = useTheme()
  const value = createMemo(() => ({
    preference: parent.preference,
    resolved: props.mode ?? parent.resolved,
    descriptor: props.descriptor ?? parent.descriptor,
  }))
  return (
    <ThemeContext.Provider value={value()}>
      <div
        data-jp-theme=""
        data-theme={value().resolved}
        data-theme-id={value().descriptor.id}
      >
        {props.children}
      </div>
    </ThemeContext.Provider>
  )
}
