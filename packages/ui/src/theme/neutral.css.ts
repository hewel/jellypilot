import { createGlobalTheme } from '@vanilla-extract/css'
import { neutralTokenValues } from './tokens'

export const neutralTokens = createGlobalTheme(':root', neutralTokenValues)
export { neutralBreakpoints } from './tokens'
