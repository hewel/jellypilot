import { uiInvariant } from '../runtime/invariant'
import type { ThemeDescriptor } from './types'

/** Build-time theme descriptor factory. No runtime CSS injection. */
export function defineTheme(input: ThemeDescriptor): ThemeDescriptor {
  uiInvariant(input.id.length > 0, 'theme-id', 'Theme descriptor requires id')
  uiInvariant(
    input.fonts?.sans && input.fonts?.mono,
    'theme-fonts',
    'Theme descriptor requires fonts.sans and fonts.mono',
  )
  uiInvariant(
    input.colors?.background && input.colors?.foreground,
    'theme-colors',
    'Theme descriptor requires colors.background and colors.foreground',
  )
  uiInvariant(
    input.space && Object.keys(input.space).length > 0,
    'theme-space',
    'Theme descriptor requires space tokens',
  )
  uiInvariant(
    input.radii && Object.keys(input.radii).length > 0,
    'theme-radii',
    'Theme descriptor requires radii tokens',
  )
  return Object.freeze({
    id: input.id,
    fonts: Object.freeze({ ...input.fonts }),
    colors: Object.freeze({ ...input.colors }),
    space: Object.freeze({ ...input.space }),
    radii: Object.freeze({ ...input.radii }),
  })
}
