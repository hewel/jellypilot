import { createContext, useContext } from 'solid-js'
import { uiInvariant } from '../runtime/invariant'
import type { ThemeContextValue } from './types'

export const ThemeContext = createContext<ThemeContextValue>()

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext)
  uiInvariant(
    value,
    'missing-uiroot',
    'useTheme requires a UIRoot ancestor for the owner document',
  )
  return value
}
