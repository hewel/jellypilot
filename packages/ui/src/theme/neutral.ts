import { defineTheme } from './defineTheme'
import { neutralTokenValues } from './tokens'

export const neutralTheme = defineTheme({
  id: 'neutral',
  fonts: {
    sans: neutralTokenValues.font.sans,
    mono: neutralTokenValues.font.mono,
  },
  colors: {
    background: neutralTokenValues.colors.background,
    foreground: neutralTokenValues.colors.foreground,
  },
  space: neutralTokenValues.space,
  radii: neutralTokenValues.radii,
})
