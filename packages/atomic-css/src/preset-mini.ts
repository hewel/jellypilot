import { createRequire } from 'node:module'
import { theme as presetTheme } from '@unocss/preset-mini'
import { PINNED_HOSTS } from './compiler/hosts.js'
import { createLayoutFamilyTable } from './schema/families.js'
import type { FamilyDescriptor } from './schema/families.js'

const require = createRequire(import.meta.url)

export const PRESET_MINI_VERSION = '66.7.4' as const

export type PresetMiniSource = {
  id: 'preset-mini'
  version: typeof PRESET_MINI_VERSION
  /** Supported selector surface for the first release (no options). */
  selectors: {
    breakpoints: Readonly<Record<string, string>>
  }
  /** Frozen supported theme snapshot for layout/spacing/sizing. */
  theme: {
    spacing: Readonly<Record<string, string>>
    width: Readonly<Record<string, string>>
    height: Readonly<Record<string, string>>
    maxWidth: Readonly<Record<string, string>>
    maxHeight: Readonly<Record<string, string>>
    minWidth: Readonly<Record<string, string>>
    minHeight: Readonly<Record<string, string>>
    zIndex: Readonly<Record<string, string>>
  }
  families: readonly FamilyDescriptor[]
}

/** No-options preset-mini adapter entry. Verifies exact package versions. */
export function presetMini(): PresetMiniSource {
  assertExactPresetVersions()
  const theme = freezeTheme()
  return {
    id: 'preset-mini',
    version: PRESET_MINI_VERSION,
    selectors: {
      breakpoints: Object.freeze({ ...presetTheme.breakpoints }),
    },
    theme,
    families: Object.freeze(createLayoutFamilyTable(theme)),
  }
}

function freezeTheme(): PresetMiniSource['theme'] {
  return {
    spacing: Object.freeze({ ...stringRecord(presetTheme.spacing) }),
    width: Object.freeze({ ...stringRecord(presetTheme.width) }),
    height: Object.freeze({ ...stringRecord(presetTheme.height) }),
    maxWidth: Object.freeze({ ...stringRecord(presetTheme.maxWidth) }),
    maxHeight: Object.freeze({ ...stringRecord(presetTheme.maxHeight) }),
    minWidth: Object.freeze({ ...stringRecord(presetTheme.minWidth) }),
    minHeight: Object.freeze({ ...stringRecord(presetTheme.minHeight) }),
    zIndex: Object.freeze({ ...stringRecord(presetTheme.zIndex) }),
  }
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
