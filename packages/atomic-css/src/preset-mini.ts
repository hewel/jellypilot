import { createRequire } from 'node:module'
import { colors, theme as presetTheme } from '@unocss/preset-mini'
import { PINNED_HOSTS } from './compiler/hosts.js'
import { flattenPresetColors } from './schema/colors.js'
import { createFamilyTable } from './schema/families.js'
import type { FamilyDescriptor } from './schema/families.js'

const require = createRequire(import.meta.url)

export const PRESET_MINI_VERSION = '66.7.4' as const

export type PresetMiniSource = {
  id: 'preset-mini'
  version: typeof PRESET_MINI_VERSION
  selectors: {
    breakpoints: Readonly<Record<string, string>>
  }
  theme: {
    spacing: Readonly<Record<string, string>>
    width: Readonly<Record<string, string>>
    height: Readonly<Record<string, string>>
    maxWidth: Readonly<Record<string, string>>
    maxHeight: Readonly<Record<string, string>>
    minWidth: Readonly<Record<string, string>>
    minHeight: Readonly<Record<string, string>>
    zIndex: Readonly<Record<string, string>>
    colors: Readonly<Record<string, string>>
    fontSize: Readonly<Record<string, string>>
    fontWeight: Readonly<Record<string, string>>
    lineHeight: Readonly<Record<string, string>>
    borderRadius: Readonly<Record<string, string>>
    boxShadow: Readonly<Record<string, string>>
  }
  unsupportedColorAliases: readonly string[]
  families: readonly FamilyDescriptor[]
}

/** No-options preset-mini adapter entry. Verifies exact package versions. */
export function presetMini(): PresetMiniSource {
  assertExactPresetVersions()
  const { tokens: flatColors, unsupportedAliases } = flattenPresetColors(
    colors as Record<string, unknown>,
  )
  const theme = freezeTheme(flatColors)
  return {
    id: 'preset-mini',
    version: PRESET_MINI_VERSION,
    selectors: {
      breakpoints: Object.freeze({ ...presetTheme.breakpoints }),
    },
    theme,
    unsupportedColorAliases: Object.freeze(unsupportedAliases),
    families: Object.freeze(createFamilyTable(theme)),
  }
}

function freezeTheme(
  flatColors: Record<string, string>,
): PresetMiniSource['theme'] {
  return {
    spacing: Object.freeze({ ...stringRecord(presetTheme.spacing) }),
    width: Object.freeze({ ...stringRecord(presetTheme.width) }),
    height: Object.freeze({ ...stringRecord(presetTheme.height) }),
    maxWidth: Object.freeze({ ...stringRecord(presetTheme.maxWidth) }),
    maxHeight: Object.freeze({ ...stringRecord(presetTheme.maxHeight) }),
    minWidth: Object.freeze({ ...stringRecord(presetTheme.minWidth) }),
    minHeight: Object.freeze({ ...stringRecord(presetTheme.minHeight) }),
    zIndex: Object.freeze({ ...stringRecord(presetTheme.zIndex) }),
    colors: Object.freeze({ ...flatColors }),
    fontSize: Object.freeze({ ...fontSizeTokens(presetTheme.fontSize) }),
    fontWeight: Object.freeze({ ...stringRecord(presetTheme.fontWeight) }),
    lineHeight: Object.freeze({ ...stringRecord(presetTheme.lineHeight) }),
    borderRadius: Object.freeze({ ...stringRecord(presetTheme.borderRadius) }),
    // Preset shared-variable shadows unsupported; only Project Theme fills this.
    boxShadow: Object.freeze({}),
  }
}

/** Font size uses only tuple element zero — never text-* line-height pairing. */
function fontSizeTokens(value: unknown): Record<string, string> {
  if (!value || typeof value !== 'object') return {}
  const out: Record<string, string> = {}
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry === 'string') {
      out[key] = entry
      continue
    }
    if (Array.isArray(entry) && typeof entry[0] === 'string') {
      out[key] = entry[0]
    }
  }
  return out
}

function stringRecord(value: unknown): Record<string, string> {
  if (!value || typeof value !== 'object') return {}
  const out: Record<string, string> = {}
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry === 'string') out[key] = entry
  }
  return out
}

function assertExactPresetVersions(): void {
  const expectedPreset = PINNED_HOSTS['@unocss/preset-mini']
  const presetVersion = readPackageVersion('@unocss/preset-mini')
  if (presetVersion !== expectedPreset) {
    throw new Error(
      `atomic-css preset version drift: @unocss/preset-mini resolved ${presetVersion}, expected ${expectedPreset}`,
    )
  }
  const coreVersion = readPackageVersion('@unocss/core')
  if (coreVersion !== undefined && coreVersion !== expectedPreset) {
    throw new Error(
      `atomic-css preset version drift: @unocss/core resolved ${coreVersion}, expected ${expectedPreset}`,
    )
  }
}

function readPackageVersion(name: string): string | undefined {
  try {
    const pkgPath = require.resolve(`${name}/package.json`)
    const pkg: unknown = require(pkgPath)
    if (
      pkg &&
      typeof pkg === 'object' &&
      'version' in pkg &&
      typeof pkg.version === 'string'
    ) {
      return pkg.version
    }
    return undefined
  } catch {
    return undefined
  }
}
