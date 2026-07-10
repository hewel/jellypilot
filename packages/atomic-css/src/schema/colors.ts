/** Flatten preset-mini nested colors to hyphenated names; DEFAULT → bare parent. */
export function flattenPresetColors(colors: Record<string, unknown>): {
  tokens: Record<string, string>
  unsupportedAliases: string[]
} {
  const tokens: Record<string, string> = {}
  const unsupportedAliases: string[] = []

  for (const [name, value] of Object.entries(colors)) {
    if (typeof value === 'string') {
      tokens[name] = value
      continue
    }
    if (!value || typeof value !== 'object') continue
    const scale = value as Record<string, unknown>
    // Legacy numeric 1-9 scales alias 100-900 in preset-mini — report unsupported.
    for (const [shade, color] of Object.entries(scale)) {
      if (typeof color !== 'string') continue
      if (shade === 'DEFAULT') {
        tokens[name] = color
        continue
      }
      if (/^[1-9]$/.test(shade)) {
        unsupportedAliases.push(`${name}-${shade}`)
        continue
      }
      tokens[`${name}-${shade}`] = color
    }
  }

  return { tokens, unsupportedAliases }
}
