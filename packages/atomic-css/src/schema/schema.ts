import { presetMini } from '../preset-mini.js'
import type { AtomicConfig, ThemeTokenRef } from '../config/types.js'
import type { FamilyDescriptor } from './families.js'

export type AtomicSchema = {
  version: '0.1.0'
  families: Map<string, FamilyDescriptor>
  breakpoints: Readonly<Record<string, string>>
  conditions: {
    dark?: string
    disabled?: string
  }
}

export type ResolvedDeclaration = {
  cssProperty: string
  value: string
}

export function buildAtomicSchema(
  config: AtomicConfig = { preset: 'preset-mini' },
): AtomicSchema {
  const source = presetMini()
  const families = new Map<string, FamilyDescriptor>()
  for (const family of source.families) {
    families.set(family.property, cloneFamily(family))
    for (const alias of family.aliases ?? []) {
      families.set(alias, cloneFamily({ ...family, property: alias }))
    }
  }

  const projectTokens = config.projectTheme?.tokens ?? {}

  for (const override of config.overrides ?? []) {
    const family = families.get(override.family)
    if (!family) {
      throw new Error(`overrideToken unknown family: ${override.family}`)
    }
    family.tokens = family.tokens ?? {}
    family.tokens[override.name] = serializeTokenValue(override.value)
  }

  for (const [name] of Object.entries(projectTokens)) {
    const spacing = families.get('padding')
    if (spacing?.tokens && name in spacing.tokens) {
      const hasOverride = (config.overrides ?? []).some(
        (entry) => entry.name === name,
      )
      if (!hasOverride) {
        throw new Error(
          `duplicate token "${name}" requires overrideToken(...) (preset and Project Theme both define it)`,
        )
      }
    }
  }

  return {
    version: '0.1.0',
    families,
    breakpoints: source.selectors.breakpoints,
    conditions: {
      dark: config.projectTheme?.conditions?.dark,
      disabled: config.projectTheme?.conditions?.disabled,
    },
  }
}

export function resolveDeclaration(
  schema: AtomicSchema,
  property: string,
  rawValue: string | number,
): ResolvedDeclaration[] {
  const family = schema.families.get(property)
  if (!family) {
    throw new Error(`unsupported atomic property: ${property}`)
  }
  const value = resolveValue(family, rawValue)
  const axis = axisExpansion(family.property, family.cssProperty)
  if (axis) {
    return axis.map((cssProperty) => ({ cssProperty, value }))
  }
  return [{ cssProperty: family.cssProperty, value }]
}

function axisExpansion(
  property: string,
  cssProperty: string,
): string[] | undefined {
  // Match preset-mini longhand expansion (not logical properties).
  if (
    property === 'paddingX' ||
    property === 'px' ||
    cssProperty === 'padding-inline'
  ) {
    return ['padding-left', 'padding-right']
  }
  if (
    property === 'paddingY' ||
    property === 'py' ||
    cssProperty === 'padding-block'
  ) {
    return ['padding-top', 'padding-bottom']
  }
  if (
    property === 'marginX' ||
    property === 'mx' ||
    cssProperty === 'margin-inline'
  ) {
    return ['margin-left', 'margin-right']
  }
  if (
    property === 'marginY' ||
    property === 'my' ||
    cssProperty === 'margin-block'
  ) {
    return ['margin-top', 'margin-bottom']
  }
  if (property === 'insetX' || cssProperty === 'inset-inline') {
    return ['left', 'right']
  }
  if (property === 'insetY' || cssProperty === 'inset-block') {
    return ['top', 'bottom']
  }
  if (property === 'gapX' || property === 'columnGap') {
    return ['column-gap']
  }
  if (property === 'gapY' || property === 'rowGap') {
    return ['row-gap']
  }
  return undefined
}

function resolveValue(
  family: FamilyDescriptor,
  rawValue: string | number,
): string {
  if (typeof rawValue === 'number') {
    if (family.categories.includes('numeric-spacing')) {
      if (rawValue < 0 && !family.allowNegativeNumeric) {
        throw new Error(`negative numeric not allowed for ${family.property}`)
      }
      return `${rawValue * 0.25}rem`
    }
    if (family.categories.includes('numeric-z')) {
      if (rawValue < 0 && !family.allowNegativeNumeric) {
        throw new Error(`negative numeric not allowed for ${family.property}`)
      }
      return String(rawValue)
    }
    throw new Error(`numeric value not allowed for ${family.property}`)
  }

  const value = rawValue

  if (value === 'full') return '100%'
  if (value === 'screen') {
    if (family.cssProperty.includes('height') || family.property === 'height') {
      return '100vh'
    }
    if (family.cssProperty.includes('width') || family.property === 'width') {
      return '100vw'
    }
    return '100vw'
  }
  if (value === 'min') return 'min-content'
  if (value === 'max') return 'max-content'
  if (value === 'fit') return 'fit-content'
  if (value === 'px') return '1px'
  if (value === 'none' && family.cssProperty.startsWith('max-')) return 'none'
  if (value === '0') return '0'

  if (family.categories.includes('native') && family.native?.includes(value)) {
    return mapNative(family.property, value)
  }

  if (
    family.categories.includes('token') &&
    family.tokens &&
    value in family.tokens
  ) {
    return family.tokens[value]!
  }

  if (family.categories.includes('fraction') && /^\d+\/\d+$/.test(value)) {
    const [a, b] = value.split('/').map(Number)
    if (!a || !b) throw new Error(`invalid fraction: ${value}`)
    return `${(a / b) * 100}%`
  }

  if (family.categories.includes('arbitrary-length') && isCssLength(value)) {
    return value
  }

  if (
    family.categories.includes('arbitrary-number') &&
    /^-?\d+(\.\d+)?$/.test(value)
  ) {
    return value
  }

  throw new Error(
    `unsupported value for ${family.property}: ${JSON.stringify(value)}`,
  )
}

function mapNative(property: string, value: string): string {
  if (property === 'display' && value === 'hidden') return 'none'
  if (property === 'alignItems' || property === 'items') {
    if (value === 'start') return 'flex-start'
    if (value === 'end') return 'flex-end'
  }
  if (property === 'justifyContent' || property === 'justify') {
    if (value === 'start') return 'flex-start'
    if (value === 'end') return 'flex-end'
    if (value === 'between') return 'space-between'
    if (value === 'around') return 'space-around'
    if (value === 'evenly') return 'space-evenly'
  }
  return value
}

function isCssLength(value: string): boolean {
  return (
    value === '0' ||
    /^-?\d+(\.\d+)?(px|rem|em|%|vh|vw|vmin|vmax|ch|ex|cm|mm|in|pt|pc)$/.test(
      value,
    ) ||
    /^calc\(.+\)$/.test(value)
  )
}

function cloneFamily(family: FamilyDescriptor): FamilyDescriptor {
  return {
    ...family,
    tokens: family.tokens ? { ...family.tokens } : undefined,
    native: family.native ? [...family.native] : undefined,
    categories: [...family.categories],
    aliases: family.aliases ? [...family.aliases] : undefined,
    conflictsWith: family.conflictsWith
      ? [...family.conflictsWith]
      : undefined,
  }
}

function serializeTokenValue(value: string | ThemeTokenRef): string {
  if (typeof value === 'string') return value
  return `var(--atomic-theme-${value.exportName})`
}
