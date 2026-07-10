import { parse } from 'css-tree'

const SEPARATOR = /[;{}]/
const IMPORTANT = /!important/i

export function assertNoForbiddenSyntax(value: string): void {
  if (IMPORTANT.test(value)) {
    throw new Error('malformed-arbitrary: !important is rejected')
  }
  if (SEPARATOR.test(value)) {
    throw new Error('malformed-arbitrary: declaration separators are rejected')
  }
}

export function validateCssValue(
  cssProperty: string,
  value: string,
  kind: 'color' | 'length' | 'number' | 'shadow' | 'grid' | 'generic',
): void {
  assertNoForbiddenSyntax(value)
  try {
    if (kind === 'color') {
      parse(value, { context: 'value' })
      if (
        !/^(#|rgb|rgba|hsl|hsla|oklch|lab|lch|color|var\(|currentColor|transparent|inherit)/i.test(
          value,
        ) &&
        !/^[a-z]+$/i.test(value)
      ) {
        // css-tree accepts many tokens; require color-ish form for arbitrary colors
        throw new Error('not a color')
      }
      return
    }
    if (kind === 'shadow') {
      parse(value, { context: 'value' })
      return
    }
    if (kind === 'grid') {
      parse(value, { context: 'value' })
      return
    }
    if (kind === 'length') {
      if (
        value !== '0' &&
        !/^-?\d+(\.\d+)?(px|rem|em|%|vh|vw|vmin|vmax|ch|ex|cm|mm|in|pt|pc)$/.test(
          value,
        ) &&
        !/^calc\(.+\)$/.test(value) &&
        !/^var\(.+\)$/.test(value)
      ) {
        throw new Error('not a length')
      }
      return
    }
    if (kind === 'number') {
      if (!/^-?\d+(\.\d+)?$/.test(value)) {
        throw new Error('not a number')
      }
      return
    }
    parse(value, { context: 'value' })
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    throw new Error(
      `malformed-arbitrary for ${cssProperty}: ${value} (${message})`,
    )
  }
}
